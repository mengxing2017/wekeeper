use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    #[serde(rename = "content.upsert")]
    Upsert,
}

/// Facts supplied only by a trusted, authenticated intake use case.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OriginalMessage {
    pub event_kind: EventKind,
    pub external_content_id: String,
    pub version: String,
    pub sequence: i64,
    pub occurred_at: chrono::DateTime<chrono::Utc>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub detail_url: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptMessage {
    pub platform_id: Uuid,
    pub unit_id: Uuid,
    pub source_event_id: Uuid,
    pub evidence_ref: Uuid,
    pub original: OriginalMessage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptedMessage {
    pub id: Uuid,
    pub facts: AcceptMessage,
    pub accepted_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    InvalidIdentity,
    Conflict,
    Unavailable,
}
pub type Result<T> = std::result::Result<T, Error>;

/// Public result projection, shared with the TypeScript client through a fixture.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MessageView {
    pub id: Uuid,
    pub platform_id: Uuid,
    pub business_unit_id: Uuid,
    pub receipt_id: Uuid,
    pub event_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub state: String,
    pub reason_code: Option<String>,
    pub content: Option<String>,
    pub decision: Option<crate::advertising::domain::decision::Decision>,
    pub sending: Option<MessageCounts>,
    pub delegates: Vec<DelegateView>,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct MessageCounts {
    pub pending: i64,
    pub sent: i64,
    pub failed: i64,
    pub unknown: i64,
}
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct DelegateView {
    pub id: Uuid,
    pub channel: String,
    pub task_id: Option<Uuid>,
    pub configuration_id: Uuid,
    pub target_count: i64,
    pub state: String,
    pub reason_code: Option<String>,
}
#[cfg(test)]
mod contract_tests {
    #[test]
    fn message_results_roundtrip_with_rust_serialization() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../api-contract/fixtures/business-message.json"
        ))
        .unwrap();
        let typed: super::MessageView = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(typed).unwrap(), fixture);
    }
}
