use super::*;
use crate::source_access::domain::projection;
#[derive(Clone)]
pub struct ReceiptsRepository(pub PgPool);
impl crate::source_access::south::port::ProjectionStore for ReceiptsRepository {
    async fn execute(
        &self,
        actor: &crate::business::Actor,
        command: projection::Command,
    ) -> Result<Value> {
        let projection::Command::List { page, .. } = &command;
        let (limit, offset) = page.fetch_bounds().map_err(|_| Fault("invalid_page"))?;
        let mut tx = self.0.begin().await?;
        super::routing::administrator(&mut tx, actor.id).await?;
        let projection::Command::List { route, .. } = &command;
        let fields = "jsonb_build_object('id',r.receipt_id,'event_id',r.event_id,'status',r.status,'reason_code',r.reason_code,'processing_stage',r.processing_stage,'received_at',r.received_at,'updated_at',r.updated_at,'revision',r.processing_revision,'sending',r.sending,'summary_version',r.summary_version,'summary_revision',r.summary_revision,'summary_total',r.summary_total)";
        let value = {
            let query = format!(
                "SELECT {fields} FROM source_receipts r WHERE r.route_id=$1 ORDER BY r.received_at DESC,r.receipt_id DESC LIMIT $2 OFFSET $3"
            );
            let rows: Vec<Value> = sqlx::query_scalar(&query)
                .bind(route)
                .bind(limit)
                .bind(offset)
                .fetch_all(&mut *tx)
                .await?;
            json!({"items":rows.iter().take(limit as usize - 1).collect::<Vec<_>>(),"has_more":rows.len() >= limit as usize})
        };
        crate::audit::record(
            &mut tx,
            actor,
            "source.receipts.read",
            actor.id,
            json!({"route_id":route}),
        )
        .await
        .map_err(|_| Fault("temporarily_unavailable"))?;
        tx.commit().await?;
        Ok(value)
    }
}
