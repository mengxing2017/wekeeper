//! Automatic-task query and control contracts. Channel executors own attempts.
use super::{Result, ensure};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub source_id: Option<Uuid>,
    pub unit_id: Option<Uuid>,
    pub route_id: Option<String>,
    pub workspace_id: Option<Uuid>,
    pub channel: Option<String>,
    pub state: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub until: Option<DateTime<Utc>>,
    pub page: Option<i64>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub expected_revision: i64,
    pub idempotency_key: Uuid,
    pub reason: String,
    #[serde(default)]
    pub target_ids: Vec<Uuid>,
}
impl Control {
    pub fn validate(&self) -> Result<()> {
        ensure(
            self.expected_revision > 0
                && !self.idempotency_key.is_nil()
                && !self.reason.trim().is_empty()
                && self.reason.chars().count() <= 2000
                && self.target_ids.len() <= 1000,
            "invalid_request",
        )
    }
}
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum VersionPolicy {
    ReplaceUnsubmitted,
    ResendNewVersion,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyInput {
    pub revision: i64,
    pub policy: VersionPolicy,
    pub reason: String,
}
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub total: i64,
    pub pending: i64,
    pub sent: i64,
    pub failed: i64,
    pub unknown: i64,
    pub stopped: i64,
    pub cancelled: i64,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskRow {
    pub id: Uuid,
    pub decision_id: Uuid,
    pub source_id: Uuid,
    pub route_id: String,
    pub unit_id: Uuid,
    pub channel: String,
    pub workspace_id: Uuid,
    pub route_revision: i64,
    pub event_id: Uuid,
    pub external_content_id: String,
    pub state: String,
    pub counts: Counts,
    pub stopped: bool,
    pub stop_reason: Option<String>,
    pub revision: i64,
    pub created_at: DateTime<Utc>,
    pub has_unknown: bool,
    pub can_stop: bool,
    pub can_retry: bool,
}
pub enum Command {
    List(Filter),
    Detail(Uuid),
    Events(Uuid, i64),
    Observations(Uuid, i64),
    Targets(Uuid),
    Attempts(Uuid, Uuid),
    Stop(Uuid, Control),
    Retry(Uuid, Control),
    Policy(String, Option<PolicyInput>),
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn task_contract_is_rust_serialized_and_strict() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/source-automation-task.json"
        ))
        .unwrap();
        let task: TaskRow = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(task).unwrap(), fixture);
        let mut invalid = fixture;
        invalid["confirmed_by"] = serde_json::json!("fabricated");
        assert!(serde_json::from_value::<TaskRow>(invalid).is_err());
    }
}
