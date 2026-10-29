use super::normalize;
use crate::business::{Result, require, text};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdentityMode {
    #[default]
    LegacyExternal,
    UiVisible,
}
impl IdentityMode {
    pub fn is_legacy(&self) -> bool {
        *self == Self::LegacyExternal
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountProfile {
    pub company_name: String,
    pub account_name: String,
    pub account_detail_text: Option<String>,
}
impl AccountProfile {
    pub fn normalized(&self) -> Result<Self> {
        Ok(Self {
            company_name: label(&self.company_name)?,
            account_name: label(&self.account_name)?,
            account_detail_text: self.account_detail_text.as_deref().map(label).transpose()?,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupProfile {
    pub group_type: String,
    pub owner_name: Option<String>,
}
impl GroupProfile {
    pub fn normalized(&self) -> Result<Self> {
        require(
            ["external", "internal"].contains(&self.group_type.as_str()),
            "群类型无效",
        )?;
        Ok(Self {
            group_type: self.group_type.clone(),
            owner_name: self.owner_name.as_deref().map(label).transpose()?,
        })
    }
}
pub fn label(s: &str) -> Result<String> {
    let s = normalize(s)?;
    text(&s, 1, 100)?;
    require(!s.contains(['\n', '\t']), "资料必须为单行文字")?;
    Ok(s)
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrategyInput {
    pub name: String,
    pub sender_account_id: Uuid,
    pub group_id: Uuid,
    pub device_id: Uuid,
    pub text_content: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "manual_once")]
    pub trigger_mode: String,
}
fn manual_once() -> String {
    "manual_once".into()
}
impl StrategyInput {
    pub fn validate(&self) -> Result<()> {
        label(&self.name)?;
        normalize(&self.text_content)?;
        require(self.trigger_mode == "manual_once", "仅支持手动执行一次")
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisibleVerification {
    pub method: String,
    pub account_profile_revision: i64,
    pub group_profile_revision: i64,
    pub evidence_ref: String,
    pub checks: Vec<String>,
}
impl VisibleVerification {
    pub fn validate(&self, a: i64, g: i64) -> Result<()> {
        require(
            self.method == "visible_ui"
                && self.account_profile_revision == a
                && self.group_profile_revision == g,
            "界面核验资料版本不匹配",
        )?;
        text(&self.evidence_ref, 1, 500)?;
        let mut checks = self.checks.clone();
        checks.sort();
        require(
            checks == ["account", "company", "group_context", "group_name"],
            "界面核验项不完整",
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_reject_external_ids_and_executable_fields() {
        assert!(
            serde_json::from_value::<GroupProfile>(
                serde_json::json!({"group_type":"external","resource_id":"x"})
            )
            .is_err()
        );
        assert!(
            serde_json::from_value::<AccountProfile>(
                serde_json::json!({"company_name":"a","account_name":"b","chat_id":"x"})
            )
            .is_err()
        );
        assert_eq!(label(" 机器人测试(2) ").unwrap(), "机器人测试(2)");
        assert!(label("a\nb").is_err());
    }
    #[test]
    fn verification_requires_every_check_and_versions() {
        let v = VisibleVerification {
            method: "visible_ui".into(),
            account_profile_revision: 1,
            group_profile_revision: 2,
            evidence_ref: "local:test".into(),
            checks: vec![
                "company".into(),
                "account".into(),
                "group_name".into(),
                "group_context".into(),
            ],
        };
        assert!(v.validate(1, 2).is_ok());
        assert!(v.validate(2, 2).is_err());
    }
}
