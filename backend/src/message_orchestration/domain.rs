use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventKind {
    #[serde(rename = "content.upsert")]
    Upsert,
    #[serde(rename = "content.withdrawn")]
    Withdrawn,
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
