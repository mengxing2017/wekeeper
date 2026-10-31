//! Transport-neutral control messages. A command ACK never authorizes sending.
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone)]
pub struct Credential {
    pub device_id: Uuid,
    pub generation: i64,
    pub token_hash: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandKind {
    TaskAvailable,
    ConfigurationChanged,
    PauseExecution,
    ResumeExecution,
    RequestStatus,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandInput {
    pub kind: CommandKind,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub id: Uuid,
    pub state: ReceiptState,
    pub result_code: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptState {
    Accepted,
    Completed,
    Rejected,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pulse {
    pub connection_id: Uuid,
    pub epoch: i64,
    pub readiness: String,
    #[serde(default)]
    pub heartbeat_id: Option<Uuid>,
    pub block_reason: Option<String>,
    #[serde(default)]
    pub receipts: Vec<Receipt>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Command {
    pub id: Uuid,
    pub sequence: i64,
    pub kind: CommandKind,
    pub state: String,
    pub resource_id: Option<Uuid>,
    pub desired_revision: Option<i64>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Frame {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub epoch: i64,
    pub protocol_version: u8,
    pub heartbeat_id: Option<Uuid>,
    pub heartbeat_interval_seconds: u16,
    pub offline_after_seconds: u16,
    pub server_time: chrono::DateTime<chrono::Utc>,
    pub desired_paused: bool,
    pub desired_revision: i64,
    pub commands: Vec<Command>,
    pub credential_expires_at: chrono::DateTime<chrono::Utc>,
}
pub enum Operation {
    Renew { candidate_hash: String },
    Activate,
    Open { transport: String },
    Pulse(Pulse),
    Poll { connection_id: Uuid, epoch: i64 },
    Close { connection_id: Uuid, epoch: i64 },
}
