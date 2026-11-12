//! Bridge committed intake evidence into independent business messages.
use crate::{
    message_orchestration::{
        domain::{AcceptMessage, AcceptedMessage, Error, EventKind, OriginalMessage},
        north::local::MessageIntake,
        south::port::MessageStore,
    },
    source_access::{domain::Fault, south::port::AcceptedEventStore},
};
use uuid::Uuid;

#[derive(Debug)]
pub enum BridgeError {
    Source(Fault),
    Orchestration(Error),
}

/// Only trusted internal callers may supply an already authorized platform.
/// Reads committed receipt evidence; retry after a crash uses the same identity.
pub struct AcceptedMessageBridge<I, S> {
    pub receipts: I,
    pub messages: MessageIntake<S>,
}
impl<I: AcceptedEventStore, S: MessageStore> AcceptedMessageBridge<I, S> {
    pub async fn accept(
        &self,
        platform_id: Uuid,
        receipt_id: Uuid,
    ) -> Result<AcceptedMessage, BridgeError> {
        let accepted = self
            .receipts
            .accepted_event(platform_id, receipt_id)
            .await
            .map_err(BridgeError::Source)?;
        let event = accepted.event;
        let kind = match event.event_type.as_str() {
            "content.upsert" => EventKind::Upsert,
            "content.withdrawn" => EventKind::Withdrawn,
            _ => return Err(BridgeError::Source(Fault("temporarily_unavailable"))),
        };
        self.messages
            .accept(AcceptMessage {
                platform_id: accepted.identity.platform_id,
                unit_id: accepted.identity.unit_id,
                source_event_id: event.event_id,
                evidence_ref: accepted.receipt_id,
                original: OriginalMessage {
                    event_kind: kind,
                    external_content_id: event.external_content_id,
                    version: event.version,
                    sequence: event.sequence,
                    occurred_at: event.occurred_at,
                    title: event.title,
                    content: event.content,
                    detail_url: event.detail_url,
                },
            })
            .await
            .map_err(BridgeError::Orchestration)
    }
}

/// Processes a bounded number of durable handoffs per scheduler tick.
pub struct HandoffWorker<Q, I, S> {
    pub queue: Q,
    pub bridge: AcceptedMessageBridge<I, S>,
}
impl<
    Q: crate::source_access::south::port::MessageHandoffStore,
    I: AcceptedEventStore,
    S: MessageStore,
> HandoffWorker<Q, I, S>
{
    pub async fn run_once(&self) -> crate::source_access::domain::Result<()> {
        for _ in 0..20 {
            let Some(claim) = self.queue.claim().await? else {
                break;
            };
            match self
                .bridge
                .accept(claim.platform_id, claim.receipt_id)
                .await
            {
                Ok(message) => self.queue.complete(&claim, message.id).await?,
                Err(error) => {
                    let code = match error {
                        BridgeError::Source(Fault("receipt_not_found")) => "receipt_not_found",
                        BridgeError::Source(Fault("evidence_mismatch")) => "evidence_mismatch",
                        BridgeError::Source(_) => "source_unavailable",
                        BridgeError::Orchestration(Error::Conflict) => "message_conflict",
                        BridgeError::Orchestration(
                            Error::InvalidInput | Error::InvalidIdentity,
                        ) => "invalid_message",
                        BridgeError::Orchestration(Error::Unavailable) => "message_unavailable",
                    };
                    self.queue.retry(&claim, code).await?;
                }
            }
        }
        Ok(())
    }
}
