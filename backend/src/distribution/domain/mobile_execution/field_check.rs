//! One target, no message body, immutable dependency-bound result.
use super::{FLOW_CHECKS, validate_text};
use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvitationInput {
    pub binding_id: Uuid,
    pub expected_binding_revision: i64,
    pub expected_target_locator_revision: i64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestInput {
    pub opening_id: Uuid,
    pub invitation_id: Uuid,
    pub expected_invitation_revision: i64,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaimInput {
    pub expected_revision: i64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FieldResultInput {
    pub challenge: String,
    pub lease_fence: i64,
    pub checks: BTreeMap<String, bool>,
    pub evidence: String,
}
impl FieldResultInput {
    pub fn validate(&self) -> Result<()> {
        require(
            self.lease_fence > 0
                && self.challenge.len() == 32
                && self.challenge.bytes().all(|b| b.is_ascii_hexdigit()),
            "检查许可无效",
        )?;
        require(
            self.checks
                .keys()
                .map(String::as_str)
                .collect::<std::collections::BTreeSet<_>>()
                == FLOW_CHECKS.iter().copied().collect(),
            "检查结果字段无效",
        )?;
        validate_text(&self.evidence, 1, 200, 200)?;
        require(
            self.evidence
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
            "检查证据代码无效",
        )
    }
    pub fn passed(&self) -> bool {
        self.checks.values().all(|v| *v)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    #[test]
    fn no_body_or_self_approval_is_accepted() {
        let request = json!({"opening_id":Uuid::from_u128(1),"invitation_id":Uuid::from_u128(2),"expected_invitation_revision":1});
        for field in ["message", "school_id", "accepted", "checks", "flow"] {
            let mut v = request.clone();
            v[field] = json!(true);
            assert!(serde_json::from_value::<RequestInput>(v).is_err());
        }
        let mut checks: BTreeMap<String, bool> =
            FLOW_CHECKS.iter().map(|k| (k.to_string(), true)).collect();
        let mut r = FieldResultInput {
            challenge: "a".repeat(32),
            lease_fence: 1,
            checks: checks.clone(),
            evidence: "native_field_check_completed".into(),
        };
        r.validate().unwrap();
        assert!(r.passed());
        r.checks.insert("no_send_performed".into(), false);
        r.validate().unwrap();
        assert!(!r.passed());
        checks.remove("chat_opened");
        r.checks = checks;
        assert!(r.validate().is_err());
        let _: Value = request;
    }
}
