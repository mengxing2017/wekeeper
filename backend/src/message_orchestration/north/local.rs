use crate::message_orchestration::{
    domain::{AcceptMessage, AcceptedMessage, Error, EventKind, Result},
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
            || input.original.external_content_id.is_empty()
            || input.original.external_content_id.len() > 256
            || input.original.version.is_empty()
            || input.original.version.len() > 256
            || !(1..=9_007_199_254_740_991).contains(&input.original.sequence)
            || input.original.detail_url.len() > 2048
            || match input.original.event_kind {
                EventKind::Upsert => {
                    input
                        .original
                        .title
                        .as_ref()
                        .is_none_or(|v| v.trim().is_empty())
                        || input
                            .original
                            .content
                            .as_ref()
                            .is_none_or(|v| v.trim().is_empty())
                }
                EventKind::Withdrawn => input.original.content.is_some(),
            }
            || input.original.title.as_ref().map_or(0, String::len)
                + input.original.content.as_ref().map_or(0, String::len)
                > 1_048_576
        {
            return Err(Error::InvalidInput);
        }
        self.0.accept(input).await
    }
}
