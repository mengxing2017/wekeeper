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
        let mut tx = self.0.begin().await?;
        let admin = super::routing::administrator(&mut tx, actor.id).await?;
        let projection::Command::List { route, page } = &command;
        super::routing::access(&mut tx, actor.id, route, admin).await?;
        let fields = "jsonb_build_object('id',r.receipt_id,'event_id',r.event_id,'status',COALESCE(c.status,r.status),'reason_code',CASE WHEN c.receipt_id IS NOT NULL THEN c.reason_code ELSE r.reason_code END,'processing_stage',r.processing_stage,'received_at',r.received_at,'updated_at',greatest(r.updated_at,c.updated_at),'revision',r.processing_revision,'observed_sequence',r.observed_sequence,'sending',COALESCE(c.sending,r.sending),'summary_version',COALESCE(c.summary_version,r.summary_version),'summary_revision',CASE WHEN c.summary_version IS NOT NULL THEN c.summary_revision ELSE r.summary_revision END,'summary_total',COALESCE(c.summary_total,r.summary_total),'external_content_id',convert_from(i.request_body,'UTF8')::jsonb #>> '{data,attributes,external_content_id}','sequence',(convert_from(i.request_body,'UTF8')::jsonb #>> '{data,attributes,sequence}')::bigint)";
        let value = {
            let query = format!(
                "SELECT {fields} FROM source_receipts r JOIN source_inbox i USING(receipt_id) LEFT JOIN source_receipts c ON c.receipt_id=r.canonical_receipt_id WHERE r.route_id=$1 ORDER BY r.received_at DESC,r.receipt_id DESC LIMIT 31 OFFSET $2"
            );
            let rows: Vec<Value> = sqlx::query_scalar(&query)
                .bind(route)
                .bind((page - 1) * 30)
                .fetch_all(&mut *tx)
                .await?;
            json!({"items":rows.iter().take(30).collect::<Vec<_>>(),"has_more":rows.len()>30})
        };
        if admin {
            crate::audit::record(
                &mut tx,
                actor,
                "source.receipts.read",
                actor.id,
                json!({"route_id":route}),
            )
            .await
            .map_err(|_| Fault("temporarily_unavailable"))?;
        }
        tx.commit().await?;
        Ok(value)
    }
}
