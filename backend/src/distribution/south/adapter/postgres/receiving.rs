//! Business-unit configuration. Workspace identifiers only partition legacy channel storage.
use crate::{
    business::{Actor, Error, Page, Result},
    distribution::{
        domain::receiving::{
            Channel, Configuration, ConfigurationInput, TargetInput, TargetResource,
        },
        south::port::receiving::Store,
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Channel {
    pub fn prefix(self) -> &'static str {
        match self {
            Self::Mobile => "mobile",
            Self::Wecom => "wecom",
        }
    }
}
async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    actor: Uuid,
    unit: Uuid,
    _write: bool,
) -> Result<()> {
    let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM accounts a JOIN business_units u ON u.id=$2 WHERE a.id=$1 AND a.active AND a.platform_admin)")
        .bind(actor).bind(unit).fetch_one(&mut **tx).await?;
    if allowed {
        Ok(())
    } else {
        Err(Error::Forbidden)
    }
}
impl Store for Repository {
    async fn target(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
        input: TargetInput,
    ) -> Result<TargetResource> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, unit, true).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,181))")
            .bind(format!("{unit}:{}", channel.as_str()))
            .execute(&mut *tx)
            .await?;
        let resource: serde_json::Value = match channel {
            Channel::Mobile => {
                sqlx::query("INSERT INTO mobile_targets(id,search_text,created_by,updated_by) VALUES($1,$2,$3,$3) ON CONFLICT(id) DO NOTHING").bind(id).bind(&input.name).bind(actor.id).execute(&mut *tx).await?;
                sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',search_text) FROM mobile_targets WHERE id=$1  AND search_text=$2 AND archived_at IS NULL").bind(id).bind(&input.name).fetch_optional(&mut *tx).await?.ok_or(Error::Conflict)?
            }
            Channel::Wecom => {
                sqlx::query("INSERT INTO wecom_groups(id,corp_id,chat_id,name,directory_group_id) SELECT $1,o.corp_id,o.key,$2,o.id  FROM wecom_asset_objects o JOIN wecom_directory_state ds ON ds.singleton WHERE o.id=$3 AND o.kind='groups' AND o.corp_id=ds.corp_id AND o.credential_generation=ds.credential_generation AND o.verification='verified' AND o.visibility='visible' ON CONFLICT DO NOTHING").bind(id).bind(&input.name).bind(input.directory_group_id).execute(&mut *tx).await?;
                sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM wecom_groups WHERE directory_group_id=$1 AND active").bind(input.directory_group_id).fetch_optional(&mut *tx).await?.ok_or(Error::Invalid("客户群尚未同步或不可用".into()))?
            }
        };
        crate::audit::record(
            &mut tx,
            actor,
            "receiving.target",
            id,
            json!({"business_unit_id":unit,"channel":channel.as_str()}),
        )
        .await?;
        tx.commit().await?;
        serde_json::from_value(resource).map_err(|_| Error::Unavailable)
    }
    async fn resources(&self, actor: &Actor, channel: Channel, unit: Uuid) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, unit, true).await?;
        let (targets, devices, senders): (Vec<Value>, Vec<Value>, Vec<Value>) = match channel {
            Channel::Mobile => (
                sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',search_text) FROM mobile_targets WHERE archived_at IS NULL ORDER BY id").fetch_all(&mut *tx).await?,
                sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM execution_devices ORDER BY id").fetch_all(&mut *tx).await?, vec![]),
            Channel::Wecom => (
                sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM wecom_groups WHERE active ORDER BY id").fetch_all(&mut *tx).await?, vec![],
                sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'name',a.username) FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.active ORDER BY a.id").fetch_all(&mut *tx).await?),
        };
        tx.commit().await?;
        let directory: Vec<Value> = if channel == Channel::Wecom {
            sqlx::query_scalar("SELECT jsonb_build_object('id',o.id,'name',COALESCE(o.data->>'name',o.key)) FROM wecom_asset_objects o JOIN wecom_directory_state ds ON ds.singleton WHERE o.kind='groups' AND o.corp_id=ds.corp_id AND o.credential_generation=ds.credential_generation AND o.verification='verified' AND o.visibility='visible' ORDER BY o.id").fetch_all(&self.0).await?
        } else {
            vec![]
        };
        Ok(
            json!({"targets":targets,"devices":devices,"senders":senders,"directory_groups":directory}),
        )
    }
    async fn save(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
        input: ConfigurationInput,
    ) -> Result<Configuration> {
        let mut tx = self.0.begin().await?;
        // Enabling this configuration is the administrator's explicit chain authorization.
        authorize(&mut tx, actor.id, unit, true).await?;
        let old: Option<Uuid> = sqlx::query_scalar(
            "SELECT business_unit_id FROM channel_receiving_configurations WHERE id=$1 FOR UPDATE",
        )
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?;
        if old.is_some_and(|v| v != unit) {
            return Err(Error::NotFound);
        }
        let table = match channel {
            Channel::Mobile => "mobile_targets",
            Channel::Wecom => "wecom_groups",
        };
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE id=ANY($1)"))
                .bind(&input.target_ids)
                .fetch_one(&mut *tx)
                .await?;
        if count != input.target_ids.len() as i64 {
            return Err(Error::NotFound);
        }
        let (table, ids) = match channel {
            Channel::Mobile => ("execution_devices", &input.device_ids),
            Channel::Wecom => ("accounts", &input.sender_ids),
        };
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT count(*) FROM {table} WHERE id=ANY($1)"))
                .bind(ids)
                .fetch_one(&mut *tx)
                .await?;
        if count != ids.len() as i64 {
            return Err(Error::NotFound);
        }
        if channel == Channel::Mobile && input.enabled {
            for target in &input.target_ids {
                for device in &input.device_ids {
                    sqlx::query("INSERT INTO mobile_device_target_bindings(id,device_id,target_id,created_by,updated_by) SELECT $1,$2,t.id,$4,$4  FROM mobile_targets t WHERE t.id=$3 ON CONFLICT(device_id,target_id) DO UPDATE SET enabled=true,updated_by=$4,updated_at=clock_timestamp()").bind(Uuid::now_v7()).bind(device).bind(target).bind(actor.id).execute(&mut *tx).await?;
                }
            }
        }
        let result = Configuration {
            id,
            business_unit_id: unit,
            name: input.name,
            enabled: input.enabled,
            target_ids: input.target_ids,
            device_ids: input.device_ids,
            sender_ids: input.sender_ids,
        };
        let written = sqlx::query("INSERT INTO channel_receiving_configurations(id,business_unit_id,channel,configuration,authorized_by) VALUES($1,$2,$3,$4,$5) ON CONFLICT(id) DO UPDATE SET configuration=$4,authorized_by=$5,updated_at=clock_timestamp() WHERE channel_receiving_configurations.business_unit_id=$2 AND channel_receiving_configurations.channel=$3")
            .bind(id).bind(unit).bind(channel.prefix()).bind(json!(result)).bind(actor.id).execute(&mut *tx).await?;
        if written.rows_affected() != 1 {
            return Err(Error::Conflict);
        }
        crate::audit::record(
            &mut tx,
            actor,
            "receiving.save",
            id,
            json!({"business_unit_id":unit,"channel":channel.prefix(),"enabled":result.enabled}),
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn list(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        page: Page,
    ) -> Result<Vec<Configuration>> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, unit, false).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let rows:Vec<Value>=sqlx::query_scalar("SELECT configuration FROM channel_receiving_configurations WHERE business_unit_id=$1 AND channel=$2 AND ($3='' OR configuration->>'name' ILIKE '%'||$3||'%') ORDER BY id LIMIT $4 OFFSET $5")
            .bind(unit).bind(channel.prefix()).bind(page.search.unwrap_or_default()).bind(limit).bind(offset).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        rows.into_iter()
            .map(|v| serde_json::from_value(v).map_err(|_| Error::Unavailable))
            .collect()
    }
    async fn get(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
    ) -> Result<Configuration> {
        let mut tx = self.0.begin().await?;
        authorize(&mut tx, actor.id, unit, false).await?;
        let value:Value=sqlx::query_scalar("SELECT configuration FROM channel_receiving_configurations WHERE business_unit_id=$1 AND channel=$2 AND id=$3").bind(unit).bind(channel.prefix()).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        tx.commit().await?;
        serde_json::from_value(value).map_err(|_| Error::Unavailable)
    }
}

/// Resolve current configuration exactly once, before durable delegation. Runtime
/// execution uses the frozen candidates and never rechecks configuration membership.
pub(crate) async fn plans(
    tx: &mut Transaction<'_, Postgres>,
    unit: Uuid,
) -> Result<Vec<crate::distribution::domain::receiving::DeliveryPlan>> {
    use crate::distribution::domain::receiving::{DeliveryPlan, DeliveryTarget};
    use sqlx::Row;
    let rows=sqlx::query("SELECT id,channel,configuration,authorized_by FROM channel_receiving_configurations WHERE business_unit_id=$1 AND (configuration->>'enabled')::boolean ORDER BY id FOR SHARE").bind(unit).fetch_all(&mut **tx).await?;
    let mut plans: Vec<DeliveryPlan> = vec![];
    // A target shared by configurations is still delivered once per channel.
    let mut seen = std::collections::HashMap::new();
    for row in rows {
        let c: Configuration =
            serde_json::from_value(row.get("configuration")).map_err(|_| Error::Unavailable)?;
        let channel = if row.get::<String, _>("channel") == "mobile" {
            Channel::Mobile
        } else {
            Channel::Wecom
        };
        {
            let resources = super::routing::targets(tx, channel.as_str()).await?;
            let mut targets = vec![];
            for resource in resources {
                let mut target: DeliveryTarget =
                    serde_json::from_value(resource).map_err(|_| Error::Unavailable)?;
                if !c.target_ids.contains(&target.id)
                    || seen.get(&(channel.as_str(), target.id)) == Some(&true)
                {
                    continue;
                }
                let key = if channel == Channel::Mobile {
                    "candidates"
                } else {
                    "senders"
                };
                let field = if channel == Channel::Mobile {
                    "device_id"
                } else {
                    "account_id"
                };
                let ids = if channel == Channel::Mobile {
                    &c.device_ids
                } else {
                    &c.sender_ids
                };
                let choices = target.identity[key]
                    .as_array_mut()
                    .ok_or(Error::Unavailable)?;
                // Explicit selections restrict authorization. Empty selections have no send resource.
                choices.retain(|v| {
                    v[field]
                        .as_str()
                        .and_then(|s| Uuid::parse_str(s).ok())
                        .is_some_and(|id| ids.contains(&id))
                });
                choices.sort_by_key(|v| {
                    ids.iter()
                        .position(|id| Some(id.to_string().as_str()) == v[field].as_str())
                        .unwrap_or(usize::MAX)
                });
                target.resource_available = !choices.is_empty();
                let key = (channel.as_str(), target.id);
                if seen.contains_key(&key) {
                    if !target.resource_available {
                        continue;
                    }
                    for plan in &mut plans {
                        if plan.channel == channel {
                            plan.targets.retain(|t| t.id != target.id);
                        }
                    }
                    targets.retain(|t: &DeliveryTarget| t.id != target.id);
                }
                seen.insert(key, target.resource_available);
                targets.push(target);
            }
            if !targets.is_empty() {
                plans.push(DeliveryPlan {
                    id: Uuid::now_v7(),
                    configuration_id: c.id,

                    channel,
                    authorized_by: row.get("authorized_by"),
                    targets,
                });
            }
        }
    }
    plans.retain(|p| !p.targets.is_empty());
    Ok(plans)
}
