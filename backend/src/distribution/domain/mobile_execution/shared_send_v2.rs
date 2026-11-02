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
    pub stage: String,
    pub between_targets_seconds: i64,
}
impl DraftInput {
    pub fn normalized(mut self) -> Result<Self> {
        require(
            matches!(self.stage.as_str(), "test_send" | "production_allowlist")
                && (1..=86400).contains(&self.between_targets_seconds),
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
    pub allowed_stages: Vec<String>,
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
                && (1..=32).contains(&self.retry_limit)
                && self.allowed_stages.len() <= 2
                && self
                    .allowed_stages
                    .iter()
                    .all(|s| matches!(s.as_str(), "test_send" | "production_allowlist")),
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

pub use super::shared_send::{
    ClaimRecovery, PoolBindingInput, PreviewItem, PreviewTarget, SchoolAccessInput,
    inspection_state, preview_blocker,
};

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
pub struct HumanSampleEvent {
    pub sample_id: Uuid,
    pub challenge: String,
    pub operation: String,
    pub elapsed_ms: i64,
    pub runtime_epoch: Uuid,
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
