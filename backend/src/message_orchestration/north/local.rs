use crate::message_orchestration::{
    domain::{AcceptMessage, AcceptedMessage, Error, Result},
    south::port::MessageStore,
};

/// Internal boundary: the caller must authenticate and authorize the source
/// before supplying these facts. This is not an external message endpoint.
pub struct MessageIntake<S>(pub S);
impl<S: MessageStore> MessageIntake<S> {
    pub async fn accept(&self, input: AcceptMessage) -> Result<AcceptedMessage> {
        if input.platform_id.is_nil()
            || input.unit_id.is_nil()
            || input.source_event_id.is_nil()
            || input.evidence_ref.is_nil()
            || input.original.content.trim().is_empty()
            || input.original.content.chars().count() > 100_000
        {
            return Err(Error::InvalidInput);
        }
        self.0.accept(input).await
    }
}
