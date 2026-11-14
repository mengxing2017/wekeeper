use crate::message_orchestration::{
    domain::{AcceptMessage, AcceptedMessage, Error, OriginalMessage, Result},
    south::port::MessageStore,
};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct Repository(pub PgPool);

impl MessageStore for Repository {
    async fn accept(&self, input: AcceptMessage) -> Result<AcceptedMessage> {
        let mut tx = self.0.begin().await.map_err(|_| Error::Unavailable)?;
        let original = serde_json::to_value(&input.original).map_err(|_| Error::Unavailable)?;
        let inserted = sqlx::query("INSERT INTO orchestration_messages(id,platform_id,unit_id,source_event_id,evidence_ref,original) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING RETURNING id,accepted_at")
            .bind(Uuid::now_v7()).bind(input.platform_id).bind(input.unit_id).bind(input.source_event_id).bind(input.evidence_ref).bind(&original)
            .fetch_optional(&mut *tx).await.map_err(|e| {
                if e.as_database_error().is_some_and(|db| db.code().as_deref()==Some("23503")) { Error::InvalidIdentity } else { Error::Unavailable }
            })?;
        if let Some(row) = inserted {
            let result = AcceptedMessage {
                id: row.get("id"),
                accepted_at: row.get("accepted_at"),
                facts: input,
            };
            tx.commit().await.map_err(|_| Error::Unavailable)?;
            return Ok(result);
        }
        // Either unique key can conflict. Both must identify the same immutable facts.
        let rows = sqlx::query("SELECT id,platform_id,unit_id,source_event_id,evidence_ref,original,accepted_at FROM orchestration_messages WHERE platform_id=$1 AND (source_event_id=$2 OR evidence_ref=$3)")
            .bind(input.platform_id).bind(input.source_event_id).bind(input.evidence_ref)
            .fetch_all(&mut *tx).await.map_err(|_| Error::Unavailable)?;
        if rows.len() != 1 {
            return Err(Error::Conflict);
        }
        let row = &rows[0];
        let facts = AcceptMessage {
            platform_id: row.get("platform_id"),
            unit_id: row.get("unit_id"),
            source_event_id: row.get("source_event_id"),
            evidence_ref: row.get("evidence_ref"),
            original: serde_json::from_value::<OriginalMessage>(row.get("original"))
                .map_err(|_| Error::Unavailable)?,
        };
        if facts != input {
            return Err(Error::Conflict);
        }
        let result = AcceptedMessage {
            id: row.get("id"),
            accepted_at: row.get("accepted_at"),
            facts,
        };
        tx.commit().await.map_err(|_| Error::Unavailable)?;
        Ok(result)
    }
}

impl Repository {
    pub async fn list(
        &self,
        actor: &crate::business::Actor,
        route: Option<String>,
        receipt: Option<Uuid>,
        page: crate::business::Page,
    ) -> crate::business::Result<serde_json::Value> {
        let (limit, offset) = page.fetch_bounds()?;
        let rows:Vec<serde_json::Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',m.id,'platform_id',m.platform_id,'business_unit_id',m.unit_id,'receipt_id',m.evidence_ref,'event_id',m.source_event_id,'created_at',m.accepted_at,'state',COALESCE(d.state,'received'),'reason_code',d.reason_code,'content',d.content,'decision',NULLIF(d.decision,'{}'::jsonb),'sending',r.sending,'delegates',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',x.id,'channel',x.channel,'task_id',x.task_id,'configuration_id',x.configuration_id,'state',x.state,'reason_code',x.reason_code,'target_count',jsonb_array_length(x.plan->'targets'))) FROM orchestration_delegates x WHERE x.message_id=m.id),'[]')) FROM orchestration_messages m JOIN source_receipts r ON r.receipt_id=m.evidence_ref JOIN accounts a ON a.id=$1 LEFT JOIN orchestration_dispatches d ON d.message_id=m.id WHERE a.active AND (a.platform_admin OR EXISTS(SELECT 1 FROM source_routes sr JOIN source_route_managers sm USING(route_id) WHERE sr.business_unit_id=m.unit_id AND sm.account_id=a.id)) AND ($2::text IS NULL OR r.route_id=$2) AND ($3::uuid IS NULL OR r.receipt_id=$3) ORDER BY m.accepted_at DESC,m.id DESC LIMIT $4 OFFSET $5")
            .bind(actor.id).bind(route).bind(receipt).bind(limit).bind(offset).fetch_all(&self.0).await?;
        let more = rows.len() >= limit as usize;
        let items: Vec<crate::message_orchestration::domain::MessageView> = rows
            .into_iter()
            .take(limit as usize - 1)
            .map(serde_json::from_value)
            .collect::<std::result::Result<_, _>>()
            .map_err(|_| crate::business::Error::Unavailable)?;
        Ok(serde_json::json!({"items":items,"has_more":more}))
    }
}
