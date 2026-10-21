use crate::{
    business::{Error, Result},
    integrations::{domain::directory::*, south::port::DirectoryStore},
};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl DirectoryStore for Repository {
    async fn configure(&self, corp: &str, scope: &str) -> Result<i64> {
        sqlx::query("INSERT INTO wecom_directory_state(singleton,corp_id,credential_scope) VALUES(TRUE,$1,$2) ON CONFLICT DO NOTHING").bind(corp).bind(scope).execute(&self.0).await?;
        let row = sqlx::query("SELECT corp_id,credential_scope,credential_generation FROM wecom_directory_state WHERE singleton").fetch_one(&self.0).await?;
        if row.get::<String, _>("corp_id") != corp {
            return Err(Error::Invalid(
                "企业配置与已有通讯录不一致，禁止混用身份".into(),
            ));
        }
        if row.get::<String, _>("credential_scope") != scope {
            return Err(Error::Invalid(
                "服务端凭据已变化，请由平台管理员验证并切换凭据代次".into(),
            ));
        }
        Ok(row.get("credential_generation"))
    }
    async fn check_rotation(
        &self,
        corp: &str,
        scope: &str,
        expected_generation: i64,
    ) -> Result<()> {
        sqlx::query("SELECT singleton FROM wecom_directory_state WHERE corp_id=$1 AND credential_scope<>$2 AND credential_generation=$3")
            .bind(corp).bind(scope).bind(expected_generation).fetch_optional(&self.0).await?.ok_or(Error::Conflict)?;
        Ok(())
    }
    async fn rotate(
        &self,
        actor: Uuid,
        corp: &str,
        scope: &str,
        expected_generation: i64,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        // All writes take the state lock first, including publication and session creation.
        let next: i64 = sqlx::query_scalar("UPDATE wecom_directory_state SET credential_scope=$2,credential_generation=credential_generation+1 WHERE singleton AND corp_id=$1 AND credential_scope<>$2 AND credential_generation=$3 RETURNING credential_generation")
            .bind(corp).bind(scope).bind(expected_generation).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        sqlx::query("UPDATE wecom_directory_sync_runs SET state='failed',error_code='credentials_rotated',finished_at=now(),lease_until=NULL WHERE state IN ('queued','running')").execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_sync_runs SET state='failed',error_code='credentials_rotated',finished_at=now(),lease_until=NULL WHERE state IN ('queued','running')").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wecom_asset_sync_items i USING wecom_asset_sync_runs r WHERE i.run_id=r.id AND r.state='failed'").execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_asset_sync_state SET freshness='unverified'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE wecom_asset_objects SET verification='unverified'")
            .execute(&mut *tx)
            .await?;
        for dataset in ["groups"] {
            sqlx::query("INSERT INTO wecom_asset_sync_runs(id,dataset,actor_id,generation) VALUES($1,$2,$3,$4)").bind(Uuid::now_v7()).bind(dataset).bind(actor).bind(next).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE wecom_members SET verification='unverified',checked_at=NULL")
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM sessions WHERE wecom_authenticated")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO wecom_directory_sync_runs(id,actor_id,generation) VALUES($1,$2,$3)",
        )
        .bind(Uuid::now_v7())
        .bind(actor)
        .bind(next)
        .execute(&mut *tx)
        .await?;
        sqlx::query("INSERT INTO audit_events(id,actor_id,action,detail) VALUES($1,$2,'wecom.credentials.rotate',$3)").bind(Uuid::now_v7()).bind(actor).bind(json!({"previous_generation":expected_generation,"generation":next})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn status(&self, corp: &str, scope: &str) -> Result<Value> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('credential_rotation_required',coalesce((SELECT corp_id=$1 AND credential_scope<>$2 FROM wecom_directory_state),false),'credential_generation',(SELECT credential_generation FROM wecom_directory_state),'snapshot_id',(SELECT snapshot_id FROM wecom_directory_state),'last_success_at',(SELECT last_success_at FROM wecom_directory_state),'latest_run',(SELECT to_jsonb(r)-'snapshot'-'lease_id'-'lease_until' FROM wecom_directory_sync_runs r ORDER BY created_at DESC,id DESC LIMIT 1))").bind(corp).bind(scope).fetch_one(&self.0).await?)
    }
    async fn enqueue(&self, actor: Option<Uuid>, interval: i64, generation: i64) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "SELECT singleton FROM wecom_directory_state WHERE credential_generation=$1 FOR UPDATE",
        )
        .bind(generation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Conflict)?;
        if let Some(v)=sqlx::query_scalar::<_,Value>("SELECT to_jsonb(r)-'snapshot'-'lease_id'-'lease_until' FROM wecom_directory_sync_runs r WHERE state IN ('queued','running')").fetch_optional(&mut *tx).await? { return Ok(v); }
        if actor.is_none() {
            let due:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM wecom_directory_sync_runs WHERE created_at>now()-$1*interval '1 second')").bind(interval).fetch_one(&mut *tx).await?;
            if !due {
                return Ok(json!({}));
            }
        }
        let id = Uuid::now_v7();
        let v=sqlx::query_scalar("INSERT INTO wecom_directory_sync_runs(id,actor_id,generation) VALUES($1,$2,$3) RETURNING to_jsonb(wecom_directory_sync_runs)-'snapshot'-'lease_id'-'lease_until'").bind(id).bind(actor).bind(generation).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id) VALUES($1,$2,'wecom.directory.sync',$3)").bind(Uuid::now_v7()).bind(actor).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(v)
    }
    async fn job(&self, id: Uuid) -> Result<Value> {
        sqlx::query_scalar("SELECT to_jsonb(r)-'snapshot'-'lease_id'-'lease_until' FROM wecom_directory_sync_runs r WHERE id=$1").bind(id).fetch_optional(&self.0).await?.ok_or(Error::NotFound)
    }
    async fn claim(&self, generation: i64) -> Result<Option<Job>> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "SELECT singleton FROM wecom_directory_state WHERE credential_generation=$1 FOR UPDATE",
        )
        .bind(generation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Conflict)?;
        let lease = Uuid::new_v4();
        let row=sqlx::query("UPDATE wecom_directory_sync_runs SET state='running',lease_id=$1,lease_until=now()+interval '2 minutes',started_at=now(),error_code=NULL WHERE id=(SELECT id FROM wecom_directory_sync_runs WHERE generation=$2 AND (state='queued' OR (state='running' AND lease_until<now())) ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) RETURNING id,started_at")
            .bind(lease).bind(generation).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        Ok(row.map(|r| Job {
            id: r.get("id"),
            lease,
            generation,
            started_at: r.get("started_at"),
        }))
    }
    async fn renew(&self, job: &Job) -> Result<()> {
        let n=sqlx::query("UPDATE wecom_directory_sync_runs SET lease_until=now()+interval '2 minutes' WHERE id=$1 AND lease_id=$2 AND state='running' AND lease_until>now() AND generation=$3 AND generation=(SELECT credential_generation FROM wecom_directory_state)").bind(job.id).bind(job.lease).bind(job.generation).execute(&self.0).await?.rows_affected();
        if n != 1 {
            return Err(Error::Conflict);
        }
        Ok(())
    }
    async fn finish(
        &self,
        job: &Job,
        departments: &[Department],
        members: &[Member],
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "SELECT singleton FROM wecom_directory_state WHERE credential_generation=$1 FOR UPDATE",
        )
        .bind(job.generation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Conflict)?;
        sqlx::query("SELECT id FROM wecom_directory_sync_runs WHERE id=$1 AND lease_id=$2 AND generation=$3 AND state='running' AND lease_until>now() FOR UPDATE").bind(job.id).bind(job.lease).bind(job.generation).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        sqlx::query("DELETE FROM wecom_departments")
            .execute(&mut *tx)
            .await?;
        for d in departments {
            sqlx::query(
                "INSERT INTO wecom_departments(id,parentid,name,\"order\") VALUES($1,$2,$3,$4)",
            )
            .bind(d.id)
            .bind(d.parentid)
            .bind(&d.name)
            .bind(d.order)
            .execute(&mut *tx)
            .await?;
        }
        for m in members {
            sqlx::query("INSERT INTO wecom_members(id,userid,name,status,department,main_department,visibility,verification,checked_at,snapshot_id) VALUES($1,$2,$3,$4,$5,$6,'visible','verified',$7,$8) ON CONFLICT(userid_key) DO UPDATE SET userid=excluded.userid,name=excluded.name,status=excluded.status,department=excluded.department,main_department=excluded.main_department,visibility='visible',verification='verified',checked_at=excluded.checked_at,snapshot_id=excluded.snapshot_id WHERE wecom_members.checked_at IS NULL OR wecom_members.checked_at<=excluded.checked_at").bind(Uuid::now_v7()).bind(&m.userid).bind(&m.name).bind(m.status).bind(&m.department).bind(m.main_department).bind(job.started_at).bind(job.id).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE wecom_members SET visibility='not_seen',verification='unverified' WHERE snapshot_id IS DISTINCT FROM $1 AND (checked_at IS NULL OR checked_at<=$2)").bind(job.id).bind(job.started_at).execute(&mut *tx).await?;
        sqlx::query("DELETE FROM sessions s USING accounts a,wecom_members m WHERE s.account_id=a.id AND a.wecom_member_id=m.id AND s.wecom_authenticated AND (m.status IS DISTINCT FROM 1 OR m.verification<>'verified')").execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_directory_state SET snapshot_id=$1,last_success_at=now()")
            .bind(job.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE wecom_directory_sync_runs SET state='succeeded',finished_at=now(),lease_until=NULL,department_count=$3,member_count=$4,snapshot=$5 WHERE id=$1 AND lease_id=$2").bind(job.id).bind(job.lease).bind(departments.len() as i32).bind(members.len() as i32).bind(json!({"departments":departments,"members":members})).execute(&mut *tx).await?;
        sqlx::query("UPDATE wecom_directory_sync_runs SET snapshot=NULL WHERE snapshot IS NOT NULL AND id NOT IN (SELECT id FROM wecom_directory_sync_runs WHERE state='succeeded' ORDER BY finished_at DESC LIMIT 2)").execute(&mut *tx).await?;
        sqlx::query("DELETE FROM wecom_directory_sync_runs WHERE state IN ('failed','succeeded') AND finished_at<now()-interval '30 days' AND snapshot IS NULL").execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn fail(&self, job: &Job, code: &str) -> Result<()> {
        sqlx::query("UPDATE wecom_directory_sync_runs SET state='failed',finished_at=now(),error_code=$3,lease_until=NULL WHERE id=$1 AND lease_id=$2 AND state='running'").bind(job.id).bind(job.lease).bind(code).execute(&self.0).await?;
        Ok(())
    }
    async fn list(&self, q: &DirectoryQuery, departments: bool) -> Result<Vec<Value>> {
        let (limit, offset) = q.pagination().fetch_bounds()?;
        if departments {
            return Ok(sqlx::query_scalar("SELECT to_jsonb(d)||jsonb_build_object('id',d.id::text) FROM wecom_departments d WHERE ($1::bigint IS NULL OR parentid=$1) ORDER BY \"order\" DESC,id LIMIT $2 OFFSET $3").bind(q.department_id).bind(limit).bind(offset).fetch_all(&self.0).await?);
        }
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',m.id,'userid',m.userid,'name',m.name,'status',m.status,'visibility',m.visibility,'verification',m.verification,'checked_at',m.checked_at,'bound_account_id',a.id,'bound_account_name',a.display_name,'departments',coalesce((SELECT jsonb_agg(to_jsonb(d) ORDER BY d.id) FROM wecom_departments d WHERE d.id=ANY(m.department)),'[]'::jsonb)) FROM wecom_members m LEFT JOIN accounts a ON a.wecom_member_id=m.id WHERE ($1::text IS NULL OR strpos(lower(m.name||' '||m.userid),lower($1))>0) AND ($2::bigint IS NULL OR $2=ANY(m.department)) AND ($3::int IS NULL OR m.status=$3) AND ($4::text IS NULL OR ($4='bound' AND a.id IS NOT NULL) OR ($4='unbound' AND a.id IS NULL)) ORDER BY m.name,m.userid_key,m.id LIMIT $5 OFFSET $6").bind(&q.search).bind(q.department_id).bind(q.status).bind(&q.binding).bind(limit).bind(offset).fetch_all(&self.0).await?)
    }
    async fn bound_userids(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar("SELECT m.userid FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.active ORDER BY m.userid_key").fetch_all(&self.0).await?)
    }
    async fn member_id(&self, id: Uuid) -> Result<String> {
        sqlx::query_scalar("SELECT userid FROM wecom_members WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.0)
            .await?
            .ok_or(Error::NotFound)
    }
    async fn account_userid(&self, id: Uuid) -> Result<String> {
        sqlx::query_scalar("SELECT m.userid FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.id=$1 AND a.active").bind(id).fetch_optional(&self.0).await?.ok_or(Error::Forbidden)
    }
    async fn observe(
        &self,
        userid: &str,
        member: Option<&Member>,
        at: chrono::DateTime<chrono::Utc>,
        generation: i64,
    ) -> Result<()> {
        let key = userid_key(userid)?;
        let mut tx = self.0.begin().await?;
        sqlx::query(
            "SELECT singleton FROM wecom_directory_state WHERE credential_generation=$1 FOR UPDATE",
        )
        .bind(generation)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Error::Conflict)?;
        if let Some(m) = member {
            sqlx::query("UPDATE wecom_members SET name=$2,status=$3,department=$4,main_department=$5,verification='verified',visibility='visible',checked_at=$6 WHERE userid_key=$1 AND (checked_at IS NULL OR checked_at<=$6)").bind(&key).bind(&m.name).bind(m.status).bind(&m.department).bind(m.main_department).bind(at).execute(&mut *tx).await?;
        } else {
            sqlx::query("UPDATE wecom_members SET verification='unavailable',checked_at=$2 WHERE userid_key=$1 AND (checked_at IS NULL OR checked_at<=$2)").bind(&key).bind(at).execute(&mut *tx).await?;
        }
        sqlx::query("DELETE FROM sessions s USING accounts a,wecom_members m WHERE s.account_id=a.id AND a.wecom_member_id=m.id AND m.userid_key=$1 AND s.wecom_authenticated AND (m.verification<>'verified' OR m.status IS DISTINCT FROM 1)").bind(key).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    async fn bind(
        &self,
        actor: Uuid,
        account: Uuid,
        member: Option<Uuid>,
        revision: i64,
        generation: Option<i64>,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        let current: i64 = sqlx::query_scalar(
            "SELECT credential_generation FROM wecom_directory_state FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        if member.is_some() && generation != Some(current) {
            return Err(Error::Conflict);
        }
        if let Some(id) = member {
            sqlx::query("SELECT id FROM wecom_members WHERE id=$1 AND verification='verified' AND status=1 AND checked_at>now()-interval '1 minute' FOR UPDATE").bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        }
        let old_member: Option<Uuid> = sqlx::query_scalar("SELECT wecom_member_id FROM accounts WHERE id=$1 AND wecom_binding_revision=$2 FOR UPDATE").bind(account).bind(revision).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        sqlx::query("UPDATE accounts SET wecom_member_id=$3,wecom_binding_revision=wecom_binding_revision+1 WHERE id=$1 AND wecom_binding_revision=$2 AND NOT platform_admin AND ($3::uuid IS NULL OR active) RETURNING id").bind(account).bind(revision).bind(member).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?;
        sqlx::query("DELETE FROM sessions WHERE account_id=$1")
            .bind(account)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id,detail) VALUES($1,$2,'wecom.binding.update',$3,$4)").bind(Uuid::now_v7()).bind(actor).bind(account).bind(json!({"member_id":member,"previous_revision":revision,"previous_member_id":old_member})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}
