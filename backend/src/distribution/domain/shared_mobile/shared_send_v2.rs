//! Shared phone sending uses its own intent, plans and v9 wire types.
use crate::business::{Error, Result, hash};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;
pub const SCHEMA: &str = "shared_mobile_send/2";
pub const SCOPE: &str = "shared_mobile_device_delegation/1";
pub fn delegation_scope(platform: Uuid, device: Uuid, generation: i64) -> Value {
    serde_json::json!({"schema":SCOPE,"platform_id":platform,"device_id":device,"credential_generation":generation,
        "package_name":"com.tencent.wework","capability":"group_text","resource_policy":"platform_managed_school_targets",
        "background_collection":true,"automatic_inspection":true,"text_input":true,"single_use_send":true})
}
/// A read observation; it never grants a lease, consent or a send permission.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExecutionStatus {
    pub phase: ExecutionPhase,
    pub observed_at: chrono::DateTime<chrono::Utc>,
    pub device_state_observed_at: Option<chrono::DateTime<chrono::Utc>>,
    pub consent_observed_at: Option<chrono::DateTime<chrono::Utc>>,
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
            "leased" | "prepared" | "invocation_issued" => Self::Executing,
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
    fn inspection_start_matches_cross_language_identity_without_approval() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/shared-mobile-send-v9.json"
        )))
        .unwrap();
        let input: InspectionStart =
            serde_json::from_value(fixture["inspection_start"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(input).unwrap(),
            fixture["inspection_start"]
        );
    }
    #[test]
    fn business_drafts_reject_retired_phase_inputs() {
        let mut value = serde_json::json!({"target_ids":[Uuid::now_v7()],"message":"通知","content_type":"text/plain","expected_revision":null,"between_targets_seconds":30});
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
            ExecutionPhase::project("prepared", true, true),
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
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeReport {
    pub runtime_epoch: Uuid,
    pub schema: String,
    pub ui_ready: bool,
    pub paused: bool,
    pub quiescent: bool,
    pub ui_context_epoch: String,
    pub conditions: std::collections::BTreeMap<String, bool>,
    pub apk_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRequest {
    pub eligible_attempt_ids: Vec<Uuid>,
    pub runtime_epoch: Uuid,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Preparation {
    pub attempt_id: Uuid,
    pub lease_fence: i64,
    pub plan_digest: String,
    pub content_digest: String,
    pub input_matches: bool,
    pub page_digest: String,
    pub flow_digest: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationInput {
    pub preparation_id: Uuid,
    pub lease_fence: i64,
    pub plan_digest: String,
    pub content_digest: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResultEvent {
    pub result_credential: Option<String>,
    pub schema: String,
    pub event_id: Uuid,
    pub claim_id: Uuid,
    pub attempt_id: Uuid,
    pub invocation_id: Option<Uuid>,
    pub credential_generation: i64,
    pub lease_fence: i64,
    pub plan_digest: String,
    pub content_digest: String,
    pub outcome: String,
    pub reason: String,
    pub consumed: bool,
    pub calling_marked: bool,
    pub evidence: String,
}
impl ResultEvent {
    pub fn validate(&self) -> Result<()> {
        require(
            self.schema == SCHEMA
                && !self.event_id.is_nil()
                && (1..=MAX_SAFE_INTEGER).contains(&self.credential_generation)
                && (1..=MAX_SAFE_INTEGER).contains(&self.lease_fence),
            "shared_send_result_invalid",
        )?;
        for digest in [&self.plan_digest, &self.content_digest] {
            require(is_digest(digest), "shared_send_digest_invalid")?;
        }
        require(
            matches!(
                self.outcome.as_str(),
                "observed_sent" | "unknown" | "failed_before_call"
            ),
            "shared_send_result_invalid",
        )?;
        require(
            self.reason.len() <= 200 && self.evidence.len() <= 2000,
            "shared_send_result_invalid",
        )?;
        require(
            !self.calling_marked || self.consumed,
            "shared_send_result_invalid",
        )?;
        if self.outcome == "unknown" {
            require(self.invocation_id.is_some(), "shared_send_result_invalid")?;
        }
        if self.outcome == "failed_before_call" {
            require(
                !self.consumed && !self.calling_marked && self.invocation_id.is_none(),
                "shared_send_unproven_not_called",
            )?;
        }
        if self.outcome == "observed_sent" {
            require(
                self.consumed
                    && self.calling_marked
                    && self.invocation_id.is_some()
                    && !self.evidence.is_empty(),
                "shared_send_observation_required",
            )?;
        }
        Ok(())
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
    pub global: Limit,
    pub school: Limit,
    pub device: Limit,
    pub target: Limit,
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        for v in [&self.global, &self.school, &self.device, &self.target] {
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
    pub inspection_seconds: i64,
    pub retry_seconds: i64,
    pub retry_limit: i64,
    pub timezone: String,
    pub allowed_windows: Vec<AllowedWindow>,
    pub limits: Limits,
    pub reason: String,
}
impl PolicyInput {
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        require(
            (60..=604800).contains(&self.confirmation_seconds)
                && (60..=86400).contains(&self.inspection_seconds)
                && (1..=86400).contains(&self.retry_seconds)
                && (1..=32).contains(&self.retry_limit),
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
            matches!(
                self.mode.as_str(),
                "disabled" | "draft_only" | "preview_only" | "enabled"
            ) && !self.reason.trim().is_empty()
                && self.reason.len() <= 2000,
            "shared_send_policy_invalid",
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentInput {
    pub delegation_id: Uuid,
    pub scope_digest: String,
    pub expected_revision: i64,
    pub accepted: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionInput {
    pub target_id: Uuid,
    pub device_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionClaim {
    pub runtime_epoch: Uuid,
    pub inspection_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionStart {
    pub inspection_id: Uuid,
    pub runtime_epoch: Uuid,
    pub lease_fence: i64,
    pub challenge: String,
    pub dependencies_digest: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionResult {
    pub checks: std::collections::BTreeMap<String, bool>,
    pub apk_sha256: String,
    pub inspection_id: Uuid,
    pub runtime_epoch: Uuid,
    pub lease_fence: i64,
    pub challenge: String,
    pub dependencies_digest: String,
    pub ui_context_epoch: String,
    pub passed: bool,
    pub evidence: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseMutation {
    pub runtime_epoch: Uuid,
    pub lease_fence: i64,
    pub quiescent: bool,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRecovery {
    pub claim_id: Uuid,
    pub recovery_credential: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SchoolAccessInput {
    pub device_id: Uuid,
    pub school_id: Uuid,
    pub expected_revision: i64,
    pub enabled: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PoolBindingInput {
    pub target_id: Uuid,
    pub device_id: Uuid,
    pub expected_access_revision: i64,
    pub search_text_override: Option<String>,
}

/// Display identity belongs to a target, even when no phone qualifies.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PreviewTarget {
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
/// Expiry takes precedence over a historic stored state.
pub fn inspection_state(
    state: Option<&str>,
    live: bool,
    eligible: bool,
    failures: i64,
    cooling: bool,
) -> &'static str {
    match (state, live) {
        (Some("passed"), true) => "passed",
        (Some("pending"), true) => "queued",
        (Some("leased"), true) => "running",
        _ if !eligible => "waiting_prerequisites",
        _ if failures >= 3 => "retry_exhausted",
        _ if cooling => "retry_wait",
        (Some("passed" | "pending" | "leased"), false) => "expired",
        (Some("failed"), _) => "failed",
        (Some("stale"), _) => "stale",
        _ => "not_scheduled",
    }
}

/// Preview and confirmation must not mistake an inspected phone for a ready phone.
pub fn preview_blocker(device: Option<Uuid>, readiness: &[Value]) -> Option<String> {
    let Some(device) = device else {
        return Some(
            if readiness.is_empty() {
                "shared_binding_missing"
            } else {
                "device_not_eligible"
            }
            .into(),
        );
    };
    match readiness
        .iter()
        .find(|r| r["device_id"] == serde_json::json!(device))
    {
        Some(r) if r["ready"] == true && r["blockers"].as_array().is_some_and(Vec::is_empty) => {
            None
        }
        Some(r) => Some(
            r["blockers"]
                .as_array()
                .and_then(|b| b.first())
                .and_then(|b| b["code"].as_str())
                .unwrap_or("device_not_eligible")
                .into(),
        ),
        None => Some("device_not_eligible".into()),
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FlowInput {
    pub name: String,
    pub flow: super::FlowV3,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineInput {
    pub flow_id: Uuid,
    pub sample_ids: Vec<Uuid>,
    pub evidence_ids: Vec<Uuid>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HumanSampleInput {
    pub flow_id: Uuid,
    pub device_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReleaseInput {
    pub name: String,
    pub version: String,
    pub flow_id: Uuid,
    pub baseline_id: Uuid,
    pub evidence_ids: Vec<Uuid>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ActivationInput {
    pub release_id: Uuid,
    pub expected_revision: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WithdrawalInput {
    pub release_id: Uuid,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ValidationInput {
    pub flow_id: Uuid,
    pub binding_id: Uuid,
    pub launch_path: String,
    pub apk_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceInput {
    pub release_id: Uuid,
    pub stage: String,
    pub apk_id: Uuid,
    pub evidence_ids: Vec<Uuid>,
    pub validation_ids: Vec<Uuid>,
    pub cases: std::collections::BTreeMap<String, bool>,
    pub soak_hours: i64,
    pub soak_passed: bool,
    pub historical_debt_closed: bool,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScopeInput {
    pub acceptance_id: Uuid,
    pub school_id: Uuid,
    pub device_ids: Vec<Uuid>,
    pub target_ids: Vec<Uuid>,
    pub auto_targets: bool,
    pub max_calls: i64,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub expected_access: std::collections::BTreeMap<Uuid, i64>,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RevocationInput {
    pub grant_id: Uuid,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportInput {
    pub device_id: Uuid,
    pub school_id: Option<Uuid>,
    pub reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupportUpdate {
    pub id: Uuid,
    pub expected_revision: i64,
    pub state: String,
    pub response: String,
    pub release_id: Option<Uuid>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "action",
    content = "input",
    rename_all = "kebab-case",
    deny_unknown_fields
)]
pub enum ManagementCommand {
    Flows(Box<FlowInput>),
    HumanSamples(HumanSampleInput),
    Baselines(BaselineInput),
    Releases(ReleaseInput),
    Activations(ActivationInput),
    Withdrawals(WithdrawalInput),
    Validations(ValidationInput),
    Acceptances(AcceptanceInput),
    Scopes(ScopeInput),
    Revocations(RevocationInput),
    Support(SupportInput),
    SupportUpdates(SupportUpdate),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settlement {
    pub event_id: Uuid,
    pub receipt_id: Uuid,
    pub runtime_epoch: Uuid,
    pub lease_fence: i64,
    pub quiescent: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub event_id: Uuid,
    pub inspection_id: Option<Uuid>,
    pub claim_id: Option<Uuid>,
    pub code: String,
    pub stage: String,
    pub details: std::collections::BTreeMap<String, String>,
}
#[derive(Clone, Debug)]
pub struct ArtifactUpload {
    pub kind: String,
    pub name: String,
    pub content: Vec<u8>,
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
