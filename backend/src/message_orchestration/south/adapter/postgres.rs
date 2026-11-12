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
