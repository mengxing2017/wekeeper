//! Shared phone sending uses its own intent, plans and v8 wire types.
use crate::business::{Error, Result, hash};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use uuid::Uuid;
pub const SCHEMA: &str = "shared_mobile_send/1";
pub const SCOPE: &str = "shared_mobile_execution_scope/2";
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DraftInput {
    pub target_ids: Vec<Uuid>,
    pub message: String,
    pub content_type: String,
    pub expected_revision: Option<i64>,
}
impl DraftInput {
    pub fn normalized(mut self) -> Result<Self> {
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
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRequest {
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
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_is_exact_except_explicit_line_endings() {
        let d = DraftInput {
            target_ids: vec![Uuid::from_u128(1)],
            message: " 空白🦦\r\n第二行 ".into(),
            content_type: "text/plain".into(),
            expected_revision: None,
        }
        .normalized()
        .unwrap();
        assert_eq!(d.message, " 空白🦦\n第二行 ");
        assert!(serde_json::from_value::<DraftInput>(serde_json::json!({"target_ids":[],"message":"x","content_type":"text/plain","device_id":Uuid::nil()})).is_err());
    }
    #[test]
    fn canonical_rejects_unsafe_wire_numbers() {
        assert!(canonical(&serde_json::json!({"fence":9007199254740992_i64})).is_err());
        assert!(canonical(&serde_json::json!({"fence":1.5})).is_err());
        assert_eq!(
            canonical(&serde_json::json!({"b":2,"a":"🦦"})).unwrap(),
            "{\"a\":\"🦦\",\"b\":2}"
        );
    }
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
    pub timezone: String,
    pub allowed_windows: Vec<AllowedWindow>,
    pub limits: Limits,
    pub reason: String,
}
impl PolicyInput {
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
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
                "disabled" | "draft_only" | "preview_only" | "test_send" | "production_allowlist"
            ) && !self.reason.trim().is_empty()
                && self.reason.len() <= 2000,
            "shared_send_policy_invalid",
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpeningInput {
    pub device_id: Uuid,
    pub release_id: Uuid,
    pub environment_snapshot_id: Uuid,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentInput {
    pub opening_id: Uuid,
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
    pub inspection_id: Uuid,
    pub runtime_epoch: Uuid,
    pub lease_fence: i64,
    pub challenge: String,
    pub dependencies_digest: String,
    pub environment_snapshot_id: Uuid,
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
#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn shared_fixture_has_exact_cross_language_hash_and_strict_fields() {
        let f: Value = serde_json::from_str(include_str!(
            "../../../../../../api-contract/fixtures/shared-mobile-send-v8.json"
        ))
        .unwrap();
        let event: ResultEvent = serde_json::from_value(f["event"].clone()).unwrap();
        event.validate().unwrap();
        assert_eq!(serde_json::to_value(&event).unwrap(), f["event"]);
        assert_eq!(digest(&f["event"]).unwrap(), f["receipt"]["body_hash"]);
        assert_eq!(hash(f["message"].as_str().unwrap()), event.content_digest);
        let mut invalid = f["event"].clone();
        invalid["device_id"] = json_uuid();
        assert!(serde_json::from_value::<ResultEvent>(invalid).is_err());
    }
    fn json_uuid() -> Value {
        Value::String(Uuid::now_v7().to_string())
    }
    #[test]
    fn result_cannot_downgrade_an_issued_call_to_not_sent() {
        let f: Value = serde_json::from_str(include_str!(
            "../../../../../../api-contract/fixtures/shared-mobile-send-v8.json"
        ))
        .unwrap();
        let mut e: ResultEvent = serde_json::from_value(f["event"].clone()).unwrap();
        e.outcome = "failed_before_call".into();
        e.consumed = false;
        e.calling_marked = false;
        assert!(e.validate().is_err());
        e.invocation_id = None;
        assert!(e.validate().is_ok());
        e.outcome = "observed_sent".into();
        assert!(e.validate().is_err());
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimRecovery {
    pub claim_id: Uuid,
    pub recovery_credential: String,
}
