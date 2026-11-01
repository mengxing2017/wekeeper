//! v7 device capability agreement. No school, sender account or inferred identity.

use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionInput {
    pub capability_id: Uuid,
    pub configuration_revision: i64,
    pub scope_digest: String,
    pub expected_revision: i64,
    pub accepted: bool,
}
impl DecisionInput {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.capability_id.is_nil()
                && self.configuration_revision > 0
                && self.expected_revision >= 0,
            "设备能力或同意版本无效",
        )?;
        require(
            self.scope_digest.len() == 64
                && self
                    .scope_digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "范围摘要无效",
        )
    }
}
pub fn scope(
    device: Uuid,
    generation: i64,
    capability: Uuid,
    revision: i64,
    digest: &str,
    name: &str,
) -> Value {
    json!({"schema":"shared_mobile_execution_scope/1","platform_source":"wekeeper",
        "device_id":device,"credential_generation":generation,"capability_id":capability,
        "configuration_revision":revision,"configuration_digest":digest,"configuration_name":name,
        "app_key":"wecom","capability":"group_text"})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_scope_matches_native_fixture_and_binds_all_execution_dependencies() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/shared-mobile-capability-context.json"
        ))
        .unwrap();
        let item = &fixture["data"]["attributes"]["scopes"][0];
        let s = &item["scope"];
        let d = Uuid::from_u128(1);
        let cap = Uuid::from_u128(2);
        let actual = scope(
            d,
            1,
            cap,
            1,
            s["configuration_digest"].as_str().unwrap(),
            "共享群文字配置🦦",
        );
        assert_eq!(actual, *s);
        assert_eq!(
            crate::business::hash(&actual.to_string()),
            item["scope_digest"].as_str().unwrap()
        );
        let flow: Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/shared-mobile-flow-v7.json"
        ))
        .unwrap();
        assert_eq!(
            crate::business::hash(&flow.to_string()),
            s["configuration_digest"]
        );
        for key in ["school_id", "grant_id", "sender_account_id"] {
            assert!(s.get(key).is_none());
        }
        for (key, value) in [
            ("credential_generation", json!(2)),
            ("configuration_revision", json!(2)),
            ("configuration_digest", json!("f".repeat(64))),
            ("device_id", json!(Uuid::from_u128(5))),
        ] {
            let mut changed = actual.clone();
            changed[key] = value;
            assert_ne!(
                crate::business::hash(&changed.to_string()),
                item["scope_digest"].as_str().unwrap()
            );
        }
        let decision = json!({"capability_id":cap,"configuration_revision":1,"scope_digest":item["scope_digest"],"expected_revision":0,"accepted":true});
        serde_json::from_value::<DecisionInput>(decision.clone())
            .unwrap()
            .validate()
            .unwrap();
        for field in ["grant_id", "school_id", "sender_account_id"] {
            let mut bad = decision.clone();
            bad[field] = json!(d);
            assert!(serde_json::from_value::<DecisionInput>(bad).is_err());
        }
        let mut invalid = decision;
        invalid["scope_digest"] = json!("A".repeat(64));
        assert!(
            serde_json::from_value::<DecisionInput>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
    }
}
