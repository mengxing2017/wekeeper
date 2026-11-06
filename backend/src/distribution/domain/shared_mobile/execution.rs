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
}
impl Completion {
    pub fn validate(&self) -> Result<()> {
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

#[cfg(test)]
mod wire_tests {
    use super::*;
    use ring::signature::{ECDSA_P256_SHA256_ASN1, UnparsedPublicKey};
    use serde_json::Value;
    #[test]
    fn fixture_preserves_result_identity_and_authenticates_the_full_grant() {
        let f: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../api-contract/fixtures/durable-execution.json"
        )))
        .unwrap();
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
