//! Global receiving resources and transactional project-owned dispatch decisions.
use crate::{
    business::{Actor, Error, Page, Result},
    distribution::{
        domain::receiving::{
            Channel, Configuration, ConfigurationInput, SelectionMode, TargetInput, TargetResource,
            reconcile_cursor, rotate, successor,
        },
        south::port::receiving::Store,
    },
};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
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
    async fn phone_progress(&self, actor: &Actor, receipt: Uuid, page: Page) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let unit: Uuid =
            sqlx::query_scalar("SELECT business_unit_id FROM source_receipts WHERE receipt_id=$1")
                .bind(receipt)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        authorize(&mut tx, actor.id, unit, false).await?;
        let (limit, offset) = page.fetch_bounds()?;
        let mut items:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(v) FROM receiving_delivery_progress v JOIN orchestration_messages m ON m.id=v.message_id WHERE m.evidence_ref=$1 ORDER BY v.id LIMIT $2 OFFSET $3")
            .bind(receipt).bind(limit).bind(offset).fetch_all(&mut *tx).await?;
        let mut has_more = items.len() as i64 == limit;
        items.truncate((limit - 1) as usize);
        let summary:Value=sqlx::query_scalar("SELECT jsonb_build_object('target_count',count(*),'expected_count',coalesce(sum(r.expected_count),0),'materialized_count',coalesce(sum(r.materialized_count),0),'sent',coalesce(sum(r.sent),0),'failed',coalesce(sum(r.failed),0),'unknown',coalesce(sum(r.unknown),0),'pending',coalesce(sum(r.pending),0)) FROM receiving_target_results r JOIN orchestration_messages m ON m.id=r.message_id WHERE m.evidence_ref=$1")
            .bind(receipt).fetch_one(&mut *tx).await?;
        let mut targets:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(r)||jsonb_build_object('target_name',t.target->>'name') FROM receiving_target_results r JOIN receiving_dispatch_targets t ON t.id=r.id JOIN orchestration_messages m ON m.id=r.message_id WHERE m.evidence_ref=$1 ORDER BY r.id LIMIT $2 OFFSET $3")
            .bind(receipt).bind(limit).bind(offset).fetch_all(&mut *tx).await?;
        has_more |= targets.len() as i64 == limit;
        targets.truncate((limit - 1) as usize);
        let view:crate::distribution::domain::receiving::PhoneProgress=serde_json::from_value(json!({"id":receipt,"items":items,"has_more":has_more,"summary":summary,"targets":targets})).map_err(|_|Error::Unavailable)?;
        tx.commit().await?;
        serde_json::to_value(view).map_err(|_| Error::Unavailable)
    }
    async fn stop_phone(
        &self,
        actor: &Actor,
        receipt: Uuid,
        target: Option<Uuid>,
    ) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        let unit: Uuid =
            sqlx::query_scalar("SELECT business_unit_id FROM source_receipts WHERE receipt_id=$1")
                .bind(receipt)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Error::NotFound)?;
        authorize(&mut tx, actor.id, unit, true).await?;
        sqlx::query("SELECT pg_advisory_xact_lock(78234610243)")
            .execute(&mut *tx)
            .await?;
        // Lock the same parent used by task creation. Delivered packages remain facts.
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT t.id FROM receiving_dispatch_targets t JOIN orchestration_messages m ON m.id=t.message_id WHERE m.evidence_ref=$1 AND t.channel='mobile' AND ($2::uuid IS NULL OR t.target_id=$2) ORDER BY t.id FOR UPDATE OF t")
            .bind(receipt).bind(target).fetch_all(&mut *tx).await?;
        sqlx::query("UPDATE receiving_dispatch_targets SET stopped=true WHERE id=ANY($1)")
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE mobile_v9_tasks SET stopped=true WHERE id IN (SELECT i.task_id FROM mobile_dispatch_intakes i JOIN receiving_dispatch_deliveries d ON d.id=i.delivery_id WHERE d.dispatch_target_id=ANY($1))").bind(&ids).execute(&mut *tx).await?;
        crate::audit::record(
            &mut tx,
            actor,
            "receiving.stop",
            receipt,
            json!({"target_id":target,"target_count":ids.len()}),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id":receipt,"stopped":true}))
    }
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
        let old = sqlx::query("SELECT configuration,next_target_id,next_device_id,revision FROM channel_receiving_configurations WHERE id=$1 FOR UPDATE")
            .bind(id).fetch_optional(&mut *tx).await?;
        let previous: Option<Configuration> = old
            .as_ref()
            .map(|r| serde_json::from_value(r.get("configuration")))
            .transpose()
            .map_err(|_| Error::Unavailable)?;
        if previous
            .as_ref()
            .is_some_and(|v| v.business_unit_id != unit)
        {
            return Err(Error::NotFound);
        }
        let next_target = reconcile_cursor(
            previous.as_ref().map_or(&[], |v| &v.target_ids),
            &input.target_ids,
            old.as_ref().and_then(|r| r.get("next_target_id")),
        );
        let next_device = reconcile_cursor(
            previous.as_ref().map_or(&[], |v| &v.device_ids),
            &input.device_ids,
            old.as_ref().and_then(|r| r.get("next_device_id")),
        );
        let revision = old.as_ref().map_or(1, |r| r.get::<i64, _>("revision") + 1);
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
            sqlx::query("INSERT INTO mobile_device_target_bindings(id,device_id,target_id,created_by,updated_by) SELECT gen_random_uuid(),d.id,t.id,$3,$3 FROM unnest($1::uuid[]) d(id) CROSS JOIN unnest($2::uuid[]) t(id) ON CONFLICT(device_id,target_id) DO NOTHING")
                .bind(&input.device_ids).bind(&input.target_ids).bind(actor.id).execute(&mut *tx).await?;
        }
        let result = Configuration {
            id,
            business_unit_id: unit,
            name: input.name,
            enabled: input.enabled,
            target_ids: input.target_ids,
            device_ids: input.device_ids,
            sender_ids: input.sender_ids,
            target_selection_mode: input.target_selection_mode,
            device_dispatch_mode: input.device_dispatch_mode,
            revision,
            next_target_id: next_target,
            next_device_id: next_device,
        };
        let written = sqlx::query("INSERT INTO channel_receiving_configurations(id,business_unit_id,channel,configuration,authorized_by,revision,next_target_id,next_device_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(id) DO UPDATE SET configuration=$4,authorized_by=$5,revision=$6,next_target_id=$7,next_device_id=$8,updated_at=clock_timestamp() WHERE channel_receiving_configurations.business_unit_id=$2 AND channel_receiving_configurations.channel=$3")
            .bind(id).bind(unit).bind(channel.prefix()).bind(json!(result)).bind(actor.id).bind(revision).bind(next_target).bind(next_device).execute(&mut *tx).await?;
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
        let rows:Vec<Value>=sqlx::query_scalar("SELECT configuration || jsonb_build_object('revision',revision,'next_target_id',next_target_id,'next_device_id',next_device_id) FROM channel_receiving_configurations WHERE business_unit_id=$1 AND channel=$2 AND ($3='' OR configuration->>'name' ILIKE '%'||$3||'%') ORDER BY id LIMIT $4 OFFSET $5")
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
        let value:Value=sqlx::query_scalar("SELECT configuration || jsonb_build_object('revision',revision,'next_target_id',next_target_id,'next_device_id',next_device_id) FROM channel_receiving_configurations WHERE business_unit_id=$1 AND channel=$2 AND id=$3").bind(unit).bind(channel.prefix()).bind(id).fetch_optional(&mut *tx).await?.ok_or(Error::NotFound)?;
        tx.commit().await?;
        serde_json::from_value(value).map_err(|_| Error::Unavailable)
    }
}

/// Allocate once, in the caller's message transaction. No feedback changes these cursors.
pub(crate) async fn plans(
    tx: &mut Transaction<'_, Postgres>,
    unit: Uuid,
    message: Uuid,
    content: &str,
) -> Result<Vec<crate::distribution::domain::receiving::DeliveryPlan>> {
    use crate::distribution::domain::receiving::{DeliveryPlan, DeliveryTarget};
    let existing: Vec<Value> = sqlx::query_scalar("SELECT plan FROM receiving_dispatch_decisions WHERE message_id=$1 ORDER BY configuration_id")
        .bind(message).fetch_all(&mut **tx).await?;
    if !existing.is_empty() {
        return existing
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(|_| Error::Unavailable))
            .collect();
    }
    let rows=sqlx::query("SELECT id,channel,configuration,authorized_by,next_target_id,next_device_id FROM channel_receiving_configurations WHERE business_unit_id=$1 AND (configuration->>'enabled')::boolean ORDER BY id FOR UPDATE")
        .bind(unit).fetch_all(&mut **tx).await?;
    let mut inputs = vec![];
    for row in &rows {
        let c: Configuration =
            serde_json::from_value(row.get("configuration")).map_err(|_| Error::Unavailable)?;
        let channel = if row.get::<String, _>("channel") == "mobile" {
            Channel::Mobile
        } else {
            Channel::Wecom
        };
        let resources = super::routing::targets(tx, channel.as_str()).await?;
        let device_order = rotate(&c.device_ids, row.get("next_device_id"));
        let target_order = if c.target_selection_mode == SelectionMode::RoundRobin {
            rotate(&c.target_ids, row.get("next_target_id"))
        } else {
            c.target_ids.clone()
        };
        let mut targets = vec![];
        for id in target_order {
            let Some(resource) = resources
                .iter()
                .find(|v| v["id"].as_str() == Some(id.to_string().as_str()))
            else {
                continue;
            };
            let mut target: DeliveryTarget =
                serde_json::from_value(resource.clone()).map_err(|_| Error::Unavailable)?;
            let (key, field, ids) = if channel == Channel::Mobile {
                ("candidates", "device_id", &device_order)
            } else {
                ("senders", "account_id", &c.sender_ids)
            };
            let choices = target.identity[key]
                .as_array_mut()
                .ok_or(Error::Unavailable)?;
            choices.retain(|v| {
                v[field]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .is_some_and(|id| ids.contains(&id))
            });
            choices.sort_by_key(|v| {
                ids.iter()
                    .position(|id| v[field].as_str() == Some(id.to_string().as_str()))
                    .unwrap_or(usize::MAX)
            });
            target.resource_available = !choices.is_empty();
            targets.push(target);
        }
        inputs.push((
            c,
            channel,
            row.get::<Uuid, _>("authorized_by"),
            device_order,
            targets,
        ));
    }
    let selection_inputs: Vec<_> = inputs
        .iter()
        .map(|(c, channel, _, _, targets)| (*channel, c.target_selection_mode, targets.clone()))
        .collect();
    let selected = crate::distribution::domain::receiving::allocate_targets(&selection_inputs);
    let mut result = vec![];
    for ((c, channel, authorized_by, devices, _), targets) in inputs.into_iter().zip(selected) {
        let plan = DeliveryPlan {
            id: Uuid::now_v7(),
            configuration_id: c.id,
            channel,
            authorized_by,
            targets,
            device_dispatch_mode: c.device_dispatch_mode,
            device_ids: devices.clone(),
            delivery_id: None,
            not_before: None,
        };
        sqlx::query("INSERT INTO receiving_dispatch_decisions(id,message_id,configuration_id,channel,configuration,plan) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(plan.id).bind(message).bind(c.id).bind(channel.as_str()).bind(json!(c)).bind(json!(plan)).execute(&mut **tx).await?;
        // Preserve the existing channel-content failure projection, without
        // reserving resources or consuming a cursor for an invalid channel.
        if !crate::advertising::domain::measure(content, channel.as_str()).valid {
            if !plan.targets.is_empty() {
                result.push(plan);
            }
            continue;
        }
        for (position, target) in plan.targets.iter().enumerate() {
            let count = if channel == Channel::Mobile
                && c.device_dispatch_mode == Some(SelectionMode::All)
            {
                c.device_ids.len()
            } else {
                1
            };
            let mut individual = plan.clone();
            individual.targets = vec![target.clone()];
            sqlx::query("INSERT INTO receiving_dispatch_targets(id,decision_id,message_id,channel,target_id,target,plan,expected_count,not_before) VALUES($1,$2,$3,$4,$5,$6,$7,$8,clock_timestamp()+make_interval(secs=>$9))")
                .bind(Uuid::now_v7()).bind(plan.id).bind(message).bind(channel.as_str()).bind(target.id).bind(json!(target)).bind(json!(individual)).bind(count as i32).bind((position*30) as f64).execute(&mut **tx).await?;
        }
        if !plan.targets.is_empty() {
            let target_next = if c.target_selection_mode == SelectionMode::RoundRobin {
                successor(&c.target_ids, plan.targets[0].id)
            } else {
                None
            };
            let device_next = if channel == Channel::Mobile
                && c.device_dispatch_mode == Some(SelectionMode::RoundRobin)
                && plan.targets.iter().any(|t| t.resource_available)
            {
                devices.first().and_then(|id| successor(&c.device_ids, *id))
            } else {
                None
            };
            sqlx::query("UPDATE channel_receiving_configurations SET next_target_id=coalesce($2,next_target_id),next_device_id=coalesce($3,next_device_id) WHERE id=$1")
                .bind(c.id).bind(target_next).bind(device_next).execute(&mut **tx).await?;
            result.push(plan);
        }
    }
    Ok(result)
}

/// Materialize at most `limit` phone responsibilities. Frozen arrays, never live configuration.
pub(crate) async fn materialize(
    pool: &PgPool,
    message: Uuid,
    limit: i32,
) -> Result<Vec<crate::distribution::domain::receiving::DeliveryPlan>> {
    use crate::distribution::domain::receiving::{DeliveryPlan, DeliveryTarget};
    let mut tx = pool.begin().await?;
    let rows=sqlx::query("SELECT t.* FROM receiving_dispatch_targets t WHERE message_id=$1 AND channel='mobile' AND materialized_count<expected_count ORDER BY id FOR UPDATE SKIP LOCKED")
        .bind(message).fetch_all(&mut *tx).await?;
    let mut remaining = limit;
    for row in rows {
        let plan: DeliveryPlan =
            serde_json::from_value(row.get("plan")).map_err(|_| Error::Unavailable)?;
        let start: i32 = row.get("materialized_count");
        let end = (start + remaining).min(row.get("expected_count"));
        for position in start..end {
            let device = if plan.device_dispatch_mode == Some(SelectionMode::All) {
                plan.device_ids.get(position as usize).copied()
            } else {
                None
            };
            sqlx::query("INSERT INTO receiving_dispatch_deliveries(id,dispatch_target_id,device_id,position) VALUES($1,$2,$3,$4) ON CONFLICT DO NOTHING")
                .bind(Uuid::now_v7()).bind(row.get::<Uuid,_>("id")).bind(device).bind(position).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE receiving_dispatch_targets SET materialized_count=$2 WHERE id=$1")
            .bind(row.get::<Uuid, _>("id"))
            .bind(end)
            .execute(&mut *tx)
            .await?;
        remaining -= end - start;
        if remaining == 0 {
            break;
        }
    }
    let rows=sqlx::query("SELECT d.id,d.device_id,t.plan,t.target,t.not_before FROM receiving_dispatch_deliveries d JOIN receiving_dispatch_targets t ON t.id=d.dispatch_target_id LEFT JOIN mobile_dispatch_intakes i ON i.delivery_id=d.id WHERE t.message_id=$1 AND NOT t.stopped AND i.id IS NULL ORDER BY d.id LIMIT $2")
        .bind(message).bind(limit as i64).fetch_all(&mut *tx).await?;
    let mut plans = vec![];
    for row in rows {
        let mut plan: DeliveryPlan =
            serde_json::from_value(row.get("plan")).map_err(|_| Error::Unavailable)?;
        let mut target: DeliveryTarget =
            serde_json::from_value(row.get("target")).map_err(|_| Error::Unavailable)?;
        if plan.device_dispatch_mode == Some(SelectionMode::All) {
            let device: Option<Uuid> = row.get("device_id");
            target.identity["candidates"]
                .as_array_mut()
                .ok_or(Error::Unavailable)?
                .retain(|c| c["device_id"].as_str().and_then(|s| s.parse::<Uuid>().ok()) == device);
            target.resource_available = !target.identity["candidates"]
                .as_array()
                .ok_or(Error::Unavailable)?
                .is_empty();
        }
        plan.targets = vec![target];
        plan.delivery_id = Some(row.get("id"));
        plan.not_before = Some(row.get("not_before"));
        plans.push(plan);
    }
    tx.commit().await?;
    Ok(plans)
}
