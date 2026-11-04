//! Device registry transactions never touch sending ledgers.
use crate::{
    business::{Error, Page, Result, hash, text},
    distribution::{domain::device_registry::*, south::port::device_registry::Store},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
type Tx<'a> = Transaction<'a, Postgres>;
mod pairing;
async fn platform(tx: &mut Tx<'_>, p: &Principal) -> Result<Uuid> {
    let id = p.actor()?;
    sqlx::query("SELECT id FROM accounts WHERE id=$1 AND active AND platform_admin FOR SHARE")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::Forbidden)?;
    Ok(id)
}
fn id(scope: &Scope) -> Result<Uuid> {
    scope.id.ok_or(Error::NotFound)
}
async fn object(tx: &mut Tx<'_>, table: &str, id: Uuid) -> Result<Value> {
    sqlx::query_scalar::<_, Value>(&format!("SELECT to_jsonb(t) FROM {table} t WHERE id=$1"))
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Error::NotFound)
}

fn revision(v: &Value, expected: i64) -> Result<()> {
    if v["revision"] == expected {
        Ok(())
    } else {
        Err(Error::Policy("revision_conflict"))
    }
}
async fn credential(tx: &mut Tx<'_>, d: Uuid) -> Result<Value> {
    let generation:i64=sqlx::query_scalar("UPDATE execution_devices SET credential_generation=credential_generation+1,revision=revision+1 WHERE id=$1 RETURNING credential_generation").bind(d).fetch_one(&mut **tx).await?;
    sqlx::query(
        "UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE device_id=$1 AND revoked_at IS NULL",
    )
    .bind(d)
    .execute(&mut **tx)
    .await?;
    let token = format!(
        "wk_device_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );
    let expires:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO device_credentials(token_hash,device_id,generation,expires_at) VALUES($1,$2,$3,clock_timestamp()+interval '30 days') RETURNING expires_at").bind(hash(&token)).bind(d).bind(generation).fetch_one(&mut **tx).await?;
    Ok(
        json!({"id":d,"device_token":token,"credential_generation":generation,"expires_at":expires,"protocol_version":9}),
    )
}
impl Repository {
    async fn apply(tx: &mut Tx<'_>, p: &Principal, s: &Scope, c: Command) -> Result<Value> {
        match c {
            Command::CreateDevice { name } => {
                platform(tx, p).await?;
                let d = Uuid::now_v7();
                sqlx::query("INSERT INTO execution_devices(id,name,enabled) VALUES($1,$2,true)")
                    .bind(d)
                    .bind(text(&name, 1, 100)?)
                    .execute(&mut **tx)
                    .await?;
                object(tx, "execution_devices", d).await
            }
            Command::UpdateDevice {
                expected_revision,
                enabled,
                reason,
            } => {
                platform(tx, p).await?;
                text(&reason, 1, 1000)?;
                let d = id(s)?;
                revision(
                    &object(tx, "execution_devices", d).await?,
                    expected_revision,
                )?;
                sqlx::query(
                    "UPDATE execution_devices SET enabled=$2,revision=revision+1 WHERE id=$1",
                )
                .bind(d)
                .bind(enabled)
                .execute(&mut **tx)
                .await?;
                object(tx, "execution_devices", d).await
            }
            Command::Enrollment { expected_revision } => {
                platform(tx, p).await?;
                let d = id(s)?;
                revision(
                    &object(tx, "execution_devices", d).await?,
                    expected_revision,
                )?;
                sqlx::query("UPDATE device_enrollments SET consumed_at=clock_timestamp() WHERE device_id=$1 AND consumed_at IS NULL").bind(d).execute(&mut **tx).await?;
                pairing::issue(tx, d).await
            }
            Command::Revoke {
                expected_revision,
                reason,
            } => {
                platform(tx, p).await?;
                text(&reason, 1, 1000)?;
                let d = id(s)?;
                revision(
                    &object(tx, "execution_devices", d).await?,
                    expected_revision,
                )?;
                sqlx::query(
                    "UPDATE device_credentials SET revoked_at=clock_timestamp() WHERE device_id=$1",
                )
                .bind(d)
                .execute(&mut **tx)
                .await?;
                sqlx::query("UPDATE execution_devices SET enabled=false,credential_generation=credential_generation+1,revision=revision+1 WHERE id=$1").bind(d).execute(&mut **tx).await?;
                object(tx, "execution_devices", d).await
            }
            Command::Enroll {
                device_id,
                code,
                client_instance_id,
                protocol_version,
            } => {
                if !matches!(p, Principal::Enrollment) || protocol_version != 9 {
                    return Err(Error::Policy("protocol_unsupported"));
                }
                let d = device_id.ok_or(Error::Policy("pairing_code_invalid"))?;
                sqlx::query("SELECT id FROM execution_devices WHERE id=$1 AND enabled FOR UPDATE")
                    .bind(d)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or(Error::Policy("device_revoked"))?;
                sqlx::query("UPDATE device_enrollments SET consumed_at=clock_timestamp() WHERE device_id=$1 AND code_hash=$2 AND consumed_at IS NULL AND expires_at>clock_timestamp() RETURNING id").bind(d).bind(hash(&code)).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("pairing_code_invalid"))?;
                sqlx::query("UPDATE execution_devices SET client_instance_id=$2,execution_protocol_version=9,supported_protocol_versions=ARRAY[9] WHERE id=$1").bind(d).bind(text(&client_instance_id,1,200)?).execute(&mut **tx).await?;
                credential(tx, d).await
            }
            Command::Rotate {} => {
                let Principal::Device { id, generation } = p else {
                    return Err(Error::Unauthorized);
                };
                sqlx::query("SELECT id FROM execution_devices WHERE id=$1 AND enabled AND credential_generation=$2 FOR UPDATE").bind(id).bind(generation).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("device_revoked"))?;
                credential(tx, *id).await
            }
        }
    }
}
impl Store for Repository {
    async fn execute(&self, p: Principal, s: Scope, key: &str, mut c: Command) -> Result<Value> {
        if matches!(c, Command::Enroll { .. }) {
            if !matches!(p, Principal::Enrollment) {
                return Err(Error::Unauthorized);
            }
            pairing::prepare(&self.0, &mut c).await?;
        }
        let mut tx = self.0.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610245)")
            .execute(&mut *tx)
            .await?;
        match &p {
            Principal::Actor(_) => {
                platform(&mut tx, &p).await?;
            }
            Principal::Enrollment => {
                if !matches!(c, Command::Enroll { .. }) {
                    return Err(Error::Unauthorized);
                }
            }
            Principal::Device { .. } => {
                if !matches!(c, Command::Rotate {}) {
                    return Err(Error::Unauthorized);
                }
            }
        }
        let value = json!(c);
        let operation = format!(
            "{}:{:?}",
            value["operation"].as_str().ok_or(Error::Conflict)?,
            s.id
        );
        let digest = hash(&value.to_string());
        let principal = if let Command::Enroll {
            device_id: Some(d), ..
        } = &c
        {
            format!("enrollment:{d}")
        } else {
            p.key()
        };
        let previous:Option<(String,Value)>=sqlx::query_as("SELECT request_hash,response FROM device_registry_operations WHERE principal=$1 AND operation=$2 AND idempotency_key=$3").bind(&principal).bind(&operation).bind(key).fetch_optional(&mut *tx).await?;
        if let Some((old, response)) = previous {
            if old != digest {
                return Err(Error::Policy("revision_conflict"));
            }
            return Ok(response);
        }
        let response = Self::apply(&mut tx, &p, &s, c).await?;
        let mut stored = response.clone();
        if let Some(obj) = stored.as_object_mut() {
            let secret = obj.remove("device_token").is_some() | obj.remove("code").is_some();
            if secret {
                obj.insert("secret_replay_unavailable".into(), json!(true));
            }
        }
        sqlx::query("INSERT INTO device_registry_operations(principal,operation,idempotency_key,request_hash,response) VALUES($1,$2,$3,$4,$5)").bind(&principal).bind(&operation).bind(key).bind(digest).bind(&stored).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO device_registry_events(id,principal,operation,device_id,detail) VALUES($1,$2,$3,$4,$5)").bind(Uuid::now_v7()).bind(principal).bind(operation).bind(s.id.or_else(||response["device_id"].as_str().or(response["id"].as_str()).and_then(|v|v.parse::<Uuid>().ok()))).bind(stored).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(response)
    }
    async fn query(&self, p: Principal, s: Scope, q: Query, page: Page) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        platform(&mut tx, &p).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let value=match q {Query::Device=>object(&mut tx,"execution_devices",id(&s)?).await?,Query::Devices=>json!(sqlx::query_scalar::<_,Value>("SELECT to_jsonb(d)||jsonb_build_object('control',to_jsonb(p)) FROM execution_devices d JOIN device_control_presence p ON p.id=d.id ORDER BY d.id LIMIT $1 OFFSET $2").bind(limit).bind(offset).fetch_all(&mut *tx).await?)};
        tx.commit().await?;
        Ok(value)
    }
}
