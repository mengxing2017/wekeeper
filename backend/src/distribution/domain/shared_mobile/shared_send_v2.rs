//! Shared phone business intent, plans, telemetry and management types.
use crate::business::{Error, Result, hash};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;
pub const SCHEMA: &str = "shared_mobile_send/3";
/// A read observation; it never grants a lease, consent or a send permission.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionStatus {
    pub phase: ExecutionPhase,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub device_state_observed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub conditions: Vec<ExecutionCondition>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionPhase {
    Waiting,
    Ready,
    Executing,
    Completed,
    Blocked,
    Unknown,
}
impl ExecutionPhase {
    pub fn project(state: &str, current: bool, waiting: bool) -> Self {
        match state {
            "queued" if !current => Self::Blocked,
            "queued" if waiting => Self::Waiting,
            "queued" => Self::Ready,
            "leased" => Self::Executing,
            "unknown" => Self::Unknown,
            "observed_sent" | "manually_confirmed_sent" => Self::Completed,
            _ => Self::Blocked,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionCondition {
    pub code: String,
    pub category: String,
    pub affected_action: String,
    pub scope_type: String,
    pub scope_id: Uuid,
    pub owner: String,
    pub message: String,
    pub automatic: bool,
    pub next_action: String,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiting_since: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason_changed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub valid_until: Option<chrono::DateTime<chrono::Utc>>,
    pub retry_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[cfg(test)]
mod execution_status_tests {
    use super::*;
    #[test]
    fn business_drafts_reject_retired_phase_inputs() {
        let mut value = serde_json::json!({"client_kind":"wecom","target_ids":[Uuid::now_v7()],"message":"通知","content_type":"text/plain","expected_revision":null,"between_targets_seconds":30});
        serde_json::from_value::<DraftInput>(value.clone())
            .unwrap()
            .normalized()
            .unwrap();
        value["stage"] = serde_json::json!("production_allowlist");
        assert!(serde_json::from_value::<DraftInput>(value).is_err());
    }
    #[test]
    fn serialization_matches_the_shared_wire_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-execution-status.json"
        )))
        .unwrap();
        let status: ExecutionStatus = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(status).unwrap(), fixture);
    }
    #[test]
    fn observations_do_not_turn_unknown_or_obsolete_attempts_into_ready_work() {
        assert_eq!(
            ExecutionPhase::project("queued", true, true),
            ExecutionPhase::Waiting
        );
        assert_eq!(
            ExecutionPhase::project("queued", true, false),
            ExecutionPhase::Ready
        );
        assert_eq!(
            ExecutionPhase::project("queued", false, false),
            ExecutionPhase::Blocked
        );
        assert_eq!(
            ExecutionPhase::project("unknown", true, false),
            ExecutionPhase::Unknown
        );
        assert_eq!(
            ExecutionPhase::project("leased", true, true),
            ExecutionPhase::Executing
        );
    }
}
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
/// No-op decisions keep checks stable; stale accepts must not undo a later withdrawal.
pub fn next_decision_revision(
    current: i64,
    expected: i64,
    accepted: Option<bool>,
    requested: bool,
) -> Result<Option<i64>> {
    require(current == expected, "revision_conflict")?;
    Ok(if accepted == Some(requested) {
        None
    } else {
        Some(current + 1)
    })
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DraftInput {
    pub client_kind: super::MobileClient,
    pub target_ids: Vec<Uuid>,
    pub message: String,
    pub content_type: String,
    pub expected_revision: Option<i64>,
    pub between_targets_seconds: i64,
}
impl DraftInput {
    pub fn normalized(mut self) -> Result<Self> {
        require(
            (1..=86400).contains(&self.between_targets_seconds),
            "shared_send_policy_invalid",
        )?;
        require(
            self.content_type == "text/plain",
            "shared_send_content_unsupported",
        )?;
        self.message = self.message.replace("\r\n", "\n").replace('\r', "\n");
        require(
            !self.message.trim().is_empty()
                && self.message.chars().count() <= 2000
                && self.message.len() <= 8000
                && !self
                    .message
                    .chars()
                    .any(|c| c.is_control() && !matches!(c, '\n' | '\t')),
            "shared_send_message_invalid",
        )?;
        require(
            !self.target_ids.is_empty()
                && self.target_ids.len() <= 100
                && self.target_ids.iter().all(|id| !id.is_nil())
                && self.target_ids.iter().collect::<BTreeSet<_>>().len() == self.target_ids.len(),
            "shared_send_targets_invalid",
        )?;
        Ok(self)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewInput {
    pub draft_id: Uuid,
    pub expected_revision: i64,
    #[serde(default)]
    pub selections: Vec<Selection>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub target_id: Uuid,
    pub device_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Confirmation {
    pub preview_id: Uuid,
    pub preview_digest: String,
    pub confirmed: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    pub expected_revision: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Review {
    pub expected_revision: i64,
    pub decision: String,
    pub evidence: String,
}
/// Optional observations, never an authorization or a delivery prerequisite.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservedFact {
    Yes,
    No,
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationLogin {
    LoggedIn,
    LoginRequired,
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentObservation {
    pub schema: String,
    pub observation_id: Uuid,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub interactive: ObservedFact,
    pub keyguard_locked: ObservedFact,
    pub device_locked: ObservedFact,
    pub keyguard_secure: ObservedFact,
    pub accessibility: ObservedFact,
    pub notification: ObservedFact,
    pub applications: std::collections::BTreeMap<String, ApplicationLogin>,
}
impl EnvironmentObservation {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema == "device_environment/2"
                && self.applications.len() == 2
                && self.applications.contains_key("wecom")
                && self.applications.contains_key("wechat")
                && !self.observation_id.is_nil()
                && self.observed_at <= chrono::Utc::now() + chrono::Duration::minutes(5),
            "send_protocol_unsupported",
        )
    }
}
pub fn is_digest(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// Wire canonicalization permits no floats, unsafe integers or non-ASCII keys.
pub fn canonical(value: &Value) -> Result<String> {
    fn validate(v: &Value) -> Result<()> {
        match v {
            Value::Number(n) => require(
                n.as_i64()
                    .is_some_and(|n| (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER).contains(&n)),
                "shared_send_unsafe_number",
            ),
            Value::Array(a) => {
                for v in a {
                    validate(v)?;
                }
                Ok(())
            }
            Value::Object(o) => {
                for (k, v) in o {
                    require(k.is_ascii(), "shared_send_invalid_key")?;
                    validate(v)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    validate(value)?;
    serde_json::to_string(value).map_err(|_| Error::Conflict)
}
pub fn digest(value: &Value) -> Result<String> {
    Ok(hash(&canonical(value)?))
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limit {
    pub interval_seconds: i64,
    pub window_seconds: i64,
    pub max_calls: i64,
    pub daily_max_calls: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub total: Limit,
    pub device: Limit,
    pub target: Limit,
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        for v in [&self.total, &self.device, &self.target] {
            require(
                (1..=86400).contains(&v.interval_seconds)
                    && (1..=2592000).contains(&v.window_seconds)
                    && (1..=100000).contains(&v.max_calls)
                    && (1..=100000).contains(&v.daily_max_calls),
                "shared_send_policy_invalid",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AllowedWindow {
    pub weekdays: Vec<i32>,
    pub start_minute: i32,
    pub end_minute: i32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyInput {
    pub expected_revision: i64,
    pub mode: String,
    pub confirmation_seconds: i64,
    pub timezone: String,
    pub allowed_windows: Vec<AllowedWindow>,
    pub limits: Limits,
    pub reason: String,
}
impl PolicyInput {
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        require(
            (60..=604800).contains(&self.confirmation_seconds),
            "shared_send_policy_invalid",
        )?;
        require(
            !self.timezone.is_empty()
                && self.timezone.len() <= 100
                && !self.allowed_windows.is_empty()
                && self.allowed_windows.len() <= 32
                && self.allowed_windows.iter().all(|w| {
                    !w.weekdays.is_empty()
                        && w.weekdays.len() <= 7
                        && w.weekdays.iter().all(|d| (1..=7).contains(d))
                        && (0..1440).contains(&w.start_minute)
                        && (0..=1440).contains(&w.end_minute)
                        && w.start_minute != w.end_minute
                }),
            "shared_send_policy_invalid",
        )?;
        require(
            matches!(self.mode.as_str(), "disabled" | "enabled")
                && !self.reason.trim().is_empty()
                && self.reason.len() <= 2000,
            "shared_send_policy_invalid",
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionInput {
    pub target_id: Uuid,
    pub device_id: Uuid,
}
pub fn require(condition: bool, code: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Policy(code))
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reassignment {
    pub expected_revision: i64,
    pub device_id: Uuid,
    pub reason: String,
}

/// Display identity belongs to a target, even when no phone qualifies.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PreviewTarget {
    pub client_kind: super::MobileClient,
    pub id: Uuid,
    pub name: String,
    pub search_text: String,
    pub locator_revision: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PreviewItem {
    pub position: usize,
    pub target_id: Uuid,
    pub target: PreviewTarget,
    pub selected: Option<Value>,
    pub candidates: Vec<Value>,
    pub readiness: Vec<Value>,
    pub blocker: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub event_id: Uuid,
    pub execution_id: Uuid,
    pub code: String,
    pub stage: String,
    pub details: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxQuery {
    pub limit: Option<i64>,
    pub cursor: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxReceiptItem {
    pub attempt_id: Uuid,
    pub assignment_revision: i64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InboxReceipt {
    pub receipt_id: Uuid,
    pub queue_revision: i64,
    pub items: Vec<InboxReceiptItem>,
}

#[cfg(test)]
mod inbox_contract_tests {
    use super::*;
    #[test]
    fn shared_inbox_fixture_preserves_execution_status_and_assignment_identity() {
        let f: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-task-inbox.json"
        )))
        .unwrap();
        let item = &f["items"][0];
        let status: ExecutionStatus =
            serde_json::from_value(item["execution_status"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            item["execution_status"]
        );
        let v = InboxReceipt {
            receipt_id: Uuid::new_v4(),
            queue_revision: f["queue_revision"].as_i64().unwrap(),
            items: vec![InboxReceiptItem {
                attempt_id: serde_json::from_value(item["attempt_id"].clone()).unwrap(),
                assignment_revision: 1,
            }],
        };
        let wire = serde_json::to_value(v).unwrap();
        let parsed: InboxReceipt = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
        assert!(item.get("message").is_none());
    }
}

/// Delivery scheduling is independent of phone execution conditions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchSnapshot {
    pub item_id: Uuid,
    pub state: String,
    pub reason: String,
    pub revision: i64,
    pub next_retry_at: Option<chrono::DateTime<chrono::Utc>>,
    pub max_rounds: i32,
    pub completed_rounds: i32,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
    pub last_failed_attempt_id: Option<Uuid>,
    pub candidates: Vec<Value>,
    pub assignments: Vec<DispatchAssignment>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchAssignment {
    pub id: Uuid,

    pub old_attempt_id: Uuid,
    pub new_attempt_id: Uuid,
    pub actor_id: Uuid,
    pub reason: String,
    pub source: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub from_device_name: Option<String>,
    pub to_device_name: Option<String>,
}
#[cfg(test)]
mod dispatch_contract_tests {
    use super::*;
    #[test]
    fn scheduling_matches_the_cross_language_fixture() {
        let value: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-dispatch.json"
        )))
        .unwrap();
        let snapshot: DispatchSnapshot = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    }
}

#[cfg(test)]
mod dispatch_failure_contract_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn failed_dispatch_has_no_retry_time_and_preserves_budget() {
        let mut value: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-dispatch.json"
        )))
        .unwrap();
        value["state"] = json!("dispatch_failed");
        value["reason"] = json!("dispatch_round_limit_exceeded");
        value["completed_rounds"] = json!(3);
        value["next_retry_at"] = Value::Null;
        value["finished_at"] = json!("2026-11-08T00:00:00Z");
        let snapshot: DispatchSnapshot = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(snapshot).unwrap(), value);
    }
}

/// Public task outcome; the manual stop flag alone does not describe results.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Dispatching,
    Completed,
    Stopped,
    DispatchFailed,
    ConfirmationExpired,
    NeedsReview,
}
#[derive(Clone, Debug, Deserialize)]
pub struct TaskTargetProgress {
    pub dispatch_state: Option<String>,
    pub attempt_state: String,
    pub sent: bool,
}
impl TaskTargetProgress {
    fn state(&self) -> &str {
        if self.sent
            || matches!(
                self.attempt_state.as_str(),
                "observed_sent" | "manually_confirmed_sent"
            )
        {
            "completed"
        } else if let Some(state) = &self.dispatch_state {
            state
        } else {
            match self.attempt_state.as_str() {
                "cancelled" => "stopped",
                "unknown" => "needs_review",
                _ => "dispatching",
            }
        }
    }
}
impl TaskStatus {
    pub fn project(stopped: bool, targets: &[TaskTargetProgress]) -> Self {
        let states: Vec<&str> = targets.iter().map(TaskTargetProgress::state).collect();
        // A stop issued after completion must not erase the completed outcome.
        if !states.is_empty() && states.iter().all(|s| *s == "completed") {
            Self::Completed
        } else if stopped {
            Self::Stopped
        } else if states.iter().any(|s| {
            !matches!(
                *s,
                "completed"
                    | "stopped"
                    | "dispatch_failed"
                    | "confirmation_expired"
                    | "needs_review"
            )
        }) {
            Self::Dispatching
        } else if states.contains(&"needs_review") {
            Self::NeedsReview
        } else if states.contains(&"dispatch_failed") {
            Self::DispatchFailed
        } else if states.contains(&"confirmation_expired") {
            Self::ConfirmationExpired
        } else if states.contains(&"stopped") {
            Self::Stopped
        } else {
            Self::Dispatching
        }
    }
}

#[cfg(test)]
mod task_status_contract_tests {
    use super::*;
    #[derive(Deserialize)]
    struct Case {
        name: String,
        stopped: bool,
        targets: Vec<TaskTargetProgress>,
        status: TaskStatus,
    }
    #[test]
    fn list_and_detail_status_precedence_matches_wire_fixture() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-task-status.json"
        )))
        .unwrap();
        let cases: Vec<Case> = serde_json::from_value(fixture.clone()).unwrap();
        for (case, wire) in cases.into_iter().zip(fixture.as_array().unwrap()) {
            let projected = TaskStatus::project(case.stopped, &case.targets);
            assert_eq!(projected, case.status, "{}", case.name);
            assert_eq!(serde_json::to_value(projected).unwrap(), wire["status"]);
        }
    }
}

#[cfg(test)]
mod environment_contract_tests {
    use super::*;
    #[test]
    fn closed_observation_round_trips_and_rejects_native_state() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/device-environment.json"
        ))
        .unwrap();
        let report: EnvironmentObservation = serde_json::from_value(fixture.clone()).unwrap();
        report.validate().unwrap();
        assert_eq!(serde_json::to_value(report).unwrap(), fixture);
        for key in [
            "paused",
            "accepted",
            "preparation",
            "ready_reason",
            "ui_ready",
            "quiescent",
        ] {
            let mut bad = fixture.clone();
            bad[key] = serde_json::json!(true);
            assert!(
                serde_json::from_value::<EnvironmentObservation>(bad).is_err(),
                "{key}"
            );
        }
        let mut bad = fixture;
        bad["interactive"] = serde_json::json!("ready");
        assert!(serde_json::from_value::<EnvironmentObservation>(bad).is_err());
    }
}
