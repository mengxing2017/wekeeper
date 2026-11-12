use super::Repository;
use crate::source_access::{
    domain::{MessageHandoff, Result},
    south::port::MessageHandoffStore,
};
use uuid::Uuid;

impl MessageHandoffStore for Repository {
    async fn claim(&self) -> Result<Option<MessageHandoff>> {
        let token = Uuid::now_v7();
        let row: Option<(Uuid,Uuid)> = sqlx::query_as("WITH candidate AS (SELECT receipt_id FROM source_message_handoffs WHERE state='pending' AND next_attempt_at<=clock_timestamp() AND (claim_until IS NULL OR claim_until<=clock_timestamp()) ORDER BY next_attempt_at,receipt_id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE source_message_handoffs h SET claim_token=$1,claim_until=clock_timestamp()+interval '60 seconds',attempts=LEAST(h.attempts+1,1000000) FROM candidate c WHERE h.receipt_id=c.receipt_id RETURNING h.receipt_id,h.platform_id")
            .bind(token).fetch_optional(&self.0).await?;
        Ok(row.map(|(receipt_id, platform_id)| MessageHandoff {
            receipt_id,
            platform_id,
            claim_token: token,
        }))
    }
    async fn complete(&self, claim: &MessageHandoff, message_id: Uuid) -> Result<()> {
        sqlx::query("UPDATE source_message_handoffs SET state='complete',message_id=$3,completed_at=clock_timestamp(),claim_token=NULL,claim_until=NULL,last_error=NULL WHERE receipt_id=$1 AND state='pending' AND claim_token=$2")
            .bind(claim.receipt_id).bind(claim.claim_token).bind(message_id).execute(&self.0).await?;
        Ok(())
    }
    async fn retry(&self, claim: &MessageHandoff, error: &'static str) -> Result<()> {
        sqlx::query("UPDATE source_message_handoffs SET claim_token=NULL,claim_until=NULL,last_error=$3,next_attempt_at=clock_timestamp()+make_interval(secs=>LEAST(300,5*power(2,LEAST(attempts,6)))::double precision) WHERE receipt_id=$1 AND state='pending' AND claim_token=$2")
            .bind(claim.receipt_id).bind(claim.claim_token).bind(error).execute(&self.0).await?;
        Ok(())
    }
}
