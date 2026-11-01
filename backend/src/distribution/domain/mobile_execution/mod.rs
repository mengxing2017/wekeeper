//! Device execution has no official customer-group or sending-account identity.
use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const PROTOCOL_VERSION: u8 = 6;
pub const SELECTION_POLICY: &str = "first_group_result";

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetInput {
    pub search_text: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub remark: String,
    #[serde(default)]
    pub duplicate_confirmation: Option<String>,
}
impl TargetInput {
    pub fn validate(&self) -> Result<()> {
        validate_text(&self.search_text, 1, 256, 1024)?;
        validate_text(&self.alias, 0, 128, 512)?;
        validate_text(&self.remark, 0, 2000, 8000)?;
        Ok(())
    }
}
pub fn validate_text(value: &str, min: usize, max: usize, bytes: usize) -> Result<()> {
    require(
        (min..=max).contains(&value.chars().count()) && value.len() <= bytes,
        "文字长度不合法",
    )?;
    require(
        value == value.trim() && !value.chars().any(char::is_control),
        "请移除首尾空白、换行或控制字符",
    )
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedBindingInput {
    pub target_id: Uuid,
    pub device_ids: Vec<Uuid>,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub search_text_override: Option<String>,
}
impl SharedBindingInput {
    pub fn validate(&self) -> Result<()> {
        require(!self.target_id.is_nil(), "请选择目标群")?;
        require(
            !self.device_ids.is_empty() && self.device_ids.len() <= 100,
            "请选择 1–100 台设备",
        )?;
        let ids: std::collections::BTreeSet<_> = self.device_ids.iter().collect();
        require(
            ids.len() == self.device_ids.len() && self.device_ids.iter().all(|id| !id.is_nil()),
            "设备不可重复或为空",
        )?;
        if let Some(value) = &self.search_text_override {
            validate_text(value, 1, 256, 1024)?;
        }
        Ok(())
    }
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SharedBindingUpdate {
    pub expected_revision: i64,
    pub enabled: bool,
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub search_text_override: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionInput {
    pub expected_revision: i64,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn locator_preserves_internal_whitespace_and_unicode() {
        let v = TargetInput {
            search_text: "家长  A群🦦".into(),
            alias: String::new(),
            remark: String::new(),
            duplicate_confirmation: None,
        };
        v.validate().unwrap();
        assert_eq!(v.search_text, "家长  A群🦦");
        for bad in [" ", " 群", "群 ", "群\n名", "群\u{0}名"] {
            assert!(validate_text(bad, 1, 256, 1024).is_err());
        }
        assert!(validate_text(&"🦦".repeat(256), 1, 256, 1024).is_ok());
        assert!(validate_text(&"🦦".repeat(257), 1, 256, 1024).is_err());
    }
    #[test]
    fn targets_reject_official_and_sender_identity_fields() {
        for field in ["wecom_group_id", "sender_account_id", "identity_mode"] {
            let value = serde_json::json!({"search_text":"群",field:"forbidden"});
            assert!(serde_json::from_value::<TargetInput>(value).is_err());
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TargetUpdate {
    pub expected_revision: i64,
    pub search_text: String,
    pub alias: String,
    pub remark: String,
    pub enabled: bool,
}

/// A device must echo the exact scope displayed at the on-device confirmation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsentInput {
    pub grant_id: Uuid,
    pub grant_revision: i64,
    pub scope_digest: String,
    pub expected_revision: i64,
    pub accepted: bool,
}
pub fn consent_scope(
    device: Uuid,
    school: Uuid,
    grant: Uuid,
    revision: i64,
    purpose: &str,
) -> serde_json::Value {
    serde_json::json!({"schema":"business_device_scope/3","platform_source":"wekeeper","device_id":device,"school_id":school,"grant_id":grant,"grant_revision":revision,"purpose":purpose,"app_key":"wecom","capability":"group_text"})
}

#[cfg(test)]
mod consent_tests {
    use super::*;
    #[test]
    fn scope_changes_with_grant_revision_purpose_school_and_device() {
        let d = Uuid::from_u128(1);
        let s = Uuid::from_u128(2);
        let g = Uuid::from_u128(3);
        let original = consent_scope(d, s, g, 1, "通知");
        assert_eq!(original["schema"], "business_device_scope/3");
        for changed in [
            consent_scope(d, s, g, 2, "通知"),
            consent_scope(d, s, g, 1, "审批"),
            consent_scope(d, Uuid::from_u128(4), g, 1, "通知"),
            consent_scope(Uuid::from_u128(4), s, g, 1, "通知"),
        ] {
            assert_ne!(
                crate::business::hash(&original.to_string()),
                crate::business::hash(&changed.to_string())
            );
        }
        assert!(serde_json::from_value::<ConsentInput>(serde_json::json!({"grant_id":g,"grant_revision":1,"scope_digest":"a","expected_revision":0,"accepted":true,"sender_account_id":d})).is_err());
    }
}

mod flow;
pub use flow::*;

#[cfg(test)]
mod shared_binding_tests {
    use super::*;
    #[test]
    fn shared_binding_preserves_override_and_rejects_inferred_school_authority() {
        let value = serde_json::json!({"target_id":Uuid::from_u128(1),"device_ids":[Uuid::from_u128(2)],"search_text_override":"同群  手机备注🦦"});
        let input: SharedBindingInput = serde_json::from_value(value.clone()).unwrap();
        input.validate().unwrap();
        assert_eq!(serde_json::to_value(&input).unwrap(), value);
        for field in ["school_id", "grant_id", "sender_account_id"] {
            let mut bad = value.clone();
            bad[field] = serde_json::json!(Uuid::from_u128(3));
            assert!(serde_json::from_value::<SharedBindingInput>(bad).is_err());
        }
        for text in ["", " ", " 群", "群\n名"] {
            let mut bad = value.clone();
            bad["search_text_override"] = serde_json::json!(text);
            assert!(
                serde_json::from_value::<SharedBindingInput>(bad)
                    .unwrap()
                    .validate()
                    .is_err()
            );
        }
        let mut inherited = value.clone();
        inherited["search_text_override"] = serde_json::Value::Null;
        serde_json::from_value::<SharedBindingInput>(inherited)
            .unwrap()
            .validate()
            .unwrap();
        let mut duplicate = value;
        duplicate["device_ids"] = serde_json::json!([Uuid::from_u128(2), Uuid::from_u128(2)]);
        assert!(
            serde_json::from_value::<SharedBindingInput>(duplicate)
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            serde_json::from_value::<TargetInput>(
                serde_json::json!({"search_text":"群","device_ids":[]})
            )
            .is_err()
        );
    }
}

pub mod shared_capability;

pub mod onboarding;

pub mod field_check;

pub mod shared_send;
