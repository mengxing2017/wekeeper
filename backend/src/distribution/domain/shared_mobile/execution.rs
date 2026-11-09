//! One grant authorizes one bounded local execution; no mid-execution network lease.
use super::shared_send_v2::require;
use crate::business::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantRequest {
    pub runtime_epoch: Uuid,
    pub eligible_attempt_ids: Vec<Uuid>,
    pub inspection_id: Option<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskDelivery {
    pub id: Uuid,
    pub schema: String,
    pub state: String,
    pub reason: Option<String>,
    pub verification_key: String,
    pub execution: Option<serde_json::Value>,
}
/// Task delivery carries the executable authorization; clients never select send attempts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryRequest {
    pub runtime_epoch: Uuid,
    pub inspection_id: Option<Uuid>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutionReceipt {
    pub execution_id: Uuid,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completion {
    pub result_id: Uuid,
    pub execution_id: Uuid,
    pub result_credential: String,
    pub outcome: String,
    pub reason: String,
    pub evidence: String,
    pub consumed: bool,
    pub calling_marked: bool,
    /// Device clocks are observational only; dispatch deadlines use database time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_started_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_finished_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl Completion {
    pub fn validate(&self) -> Result<()> {
        require(
            self.device_started_at
                .zip(self.device_finished_at)
                .is_none_or(|(a, b)| a <= b),
            "execution_device_time_invalid",
        )?;
        require(
            matches!(
                self.outcome.as_str(),
                "observed_sent"
                    | "not_called"
                    | "failed"
                    | "unknown"
                    | "inspection_passed"
                    | "inspection_failed"
            ),
            "execution_outcome_invalid",
        )?;
        require(
            self.result_credential.len() == 64
                && self.reason.len() <= 200
                && self.evidence.len() <= 2000,
            "execution_result_invalid",
        )?;
        require(
            !self.calling_marked || self.consumed,
            "execution_call_invalid",
        )?;
        require(
            self.outcome != "observed_sent"
                || (self.consumed && self.calling_marked && !self.evidence.is_empty()),
            "execution_observation_required",
        )?;
        require(
            !matches!(self.outcome.as_str(), "not_called" | "failed") || !self.calling_marked,
            "execution_call_unknown",
        )
    }
}
/// Result waiting is independent of control connections and individual request lifetimes.
pub fn replacement_reason(
    online: bool,
    outcome: Option<&str>,
    deadline_reached: bool,
) -> Option<&'static str> {
    if matches!(outcome, Some("observed_sent" | "inspection_passed")) {
        return None;
    }
    if !online {
        Some("device_offline")
    } else if matches!(outcome, Some("not_called" | "failed" | "inspection_failed")) {
        Some("execution_failed")
    } else if deadline_reached {
        Some("result_timeout")
    } else {
        None
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn three_complete_rounds_are_terminal() {
        assert!(!round_exhausted(0));
        assert!(!round_exhausted(1));
        assert!(round_exhausted(2));
        assert!(dispatch_terminal("dispatch_failed"));
        assert!(!dispatch_terminal("waiting_resources"));
    }
    #[test]
    fn delivery_request_does_not_accept_downstream_candidate_selection() {
        let runtime = Uuid::new_v4();
        let request = serde_json::json!({"runtime_epoch":runtime,"inspection_id":null});
        let parsed: DeliveryRequest = serde_json::from_value(request.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), request);
        let mut legacy = request;
        legacy["eligible_attempt_ids"] = serde_json::json!([]);
        assert!(serde_json::from_value::<DeliveryRequest>(legacy).is_err());
    }
    #[test]
    fn dispatch_does_not_wait_for_old_device_or_unknown_review() {
        assert_eq!(
            replacement_reason(false, None, false),
            Some("device_offline")
        );
        assert_eq!(
            replacement_reason(true, Some("failed"), false),
            Some("execution_failed")
        );
        assert_eq!(replacement_reason(true, Some("unknown"), false), None);
        assert_eq!(
            replacement_reason(true, Some("unknown"), true),
            Some("result_timeout")
        );
        assert_eq!(replacement_reason(false, Some("observed_sent"), true), None);
    }
}

pub const MAX_DISPATCH_ROUNDS: i32 = 3;

pub fn dispatch_terminal(state: &str) -> bool {
    matches!(
        state,
        "completed" | "stopped" | "confirmation_expired" | "dispatch_failed"
    )
}

/// A round is charged only after all candidates (including an outstanding assignment) end.
pub fn round_exhausted(completed_rounds: i32) -> bool {
    completed_rounds + 1 >= MAX_DISPATCH_ROUNDS
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};
    use serde_json::Value;
    #[test]
    fn optional_device_times_preserve_legacy_payload_and_validate_order() {
        let fixture: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/durable-execution.json"
        )))
        .unwrap();
        let legacy: Completion = serde_json::from_value(fixture["completion"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(&legacy).unwrap(),
            fixture["completion"]
        );
        let mut timed = legacy;
        timed.device_started_at = Some("2020-01-01T00:00:00Z".parse().unwrap());
        timed.device_finished_at = Some("2020-01-01T00:00:01Z".parse().unwrap());
        timed.validate().unwrap();
        let encoded = serde_json::to_value(&timed).unwrap();
        assert_eq!(encoded["device_finished_at"], "2020-01-01T00:00:01Z");
        std::mem::swap(&mut timed.device_started_at, &mut timed.device_finished_at);
        assert!(timed.validate().is_err());
    }
    #[test]
    fn fixture_preserves_result_identity_and_authenticates_the_full_grant() {
        let f: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/durable-execution.json"
        )))
        .unwrap();
        let delivery: TaskDelivery = serde_json::from_value(f["delivery"].clone()).unwrap();
        assert_eq!(delivery.schema, "task_delivery/1");
        assert_eq!(delivery.execution.as_ref().unwrap(), &f["grant"]);
        assert_eq!(delivery.verification_key, f["verification_key"]);
        assert_eq!(serde_json::to_value(delivery).unwrap(), f["delivery"]);
        let event: Completion = serde_json::from_value(f["completion"].clone()).unwrap();
        event.validate().unwrap();
        assert_eq!(serde_json::to_value(event).unwrap(), f["completion"]);
        let decode = |s: &str| {
            (0..s.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>()
        };
        let key = decode(f["verification_key"].as_str().unwrap());
        let signature = decode(f["grant"]["signature"].as_str().unwrap());
        let mut grant = f["grant"].clone();
        grant.as_object_mut().unwrap().remove("signature");
        grant.as_object_mut().unwrap().remove("key_id");
        let body = super::super::shared_send_v2::canonical(&grant).unwrap();
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &key)
            .verify(body.as_bytes(), &signature)
            .unwrap();
        grant["local_budget_ms"] = serde_json::json!(300000);
        assert!(
            UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &key)
                .verify(
                    super::super::shared_send_v2::canonical(&grant)
                        .unwrap()
                        .as_bytes(),
                    &signature
                )
                .is_err()
        );
    }
}
