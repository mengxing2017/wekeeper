//! Business schemes are unit scoped; execution transport remains unchanged.
use super::shared_send_v2::require;
use crate::business::Result;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
/// Business-only policy: technical stages and execution timeouts are not administrator inputs.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BusinessSchemeInput {
    pub expected_revision: i64,
    pub enabled: bool,
    pub confirmed: bool,
    pub timezone: String,
    pub allowed_windows: Vec<super::shared_send_v2::AllowedWindow>,
    pub limits: super::shared_send_v2::Limits,
    pub reason: String,
}
impl BusinessSchemeInput {
    pub fn policy(self) -> Result<super::shared_send_v2::PolicyInput> {
        require(
            self.confirmed && self.expected_revision >= 0,
            "confirmation_required",
        )?;
        let p = super::shared_send_v2::PolicyInput {
            expected_revision: self.expected_revision,
            mode: if self.enabled { "enabled" } else { "disabled" }.into(),
            confirmation_seconds: 86400,
            timezone: self.timezone,
            allowed_windows: self.allowed_windows,
            limits: self.limits,
            reason: self.reason,
        };
        p.validate()?;
        Ok(p)
    }
}
/// Platform permission describes business settings only; device readiness is per binding.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendingStatus {
    pub platform: String,
    pub can_confirm: bool,
    pub owner: String,
    pub message: String,
}
impl SendingStatus {
    pub fn project(mode: Option<&str>) -> Self {
        let platform = match mode {
            None => "default",
            Some("enabled") => "enabled",
            _ => "disabled",
        };
        let (owner, message) = match platform {
            "enabled" => ("admin", "当前业务方案允许执行，平台直接交付授权手机"),
            "default" => ("admin", "使用系统默认发送设置，可正常提交任务"),
            _ => ("platform", "平台已暂停实际执行，任务仍可提交排队"),
        };
        Self {
            platform: platform.into(),
            can_confirm: true,
            owner: owner.into(),
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemeMutation {
    pub expected_revision: i64,
    pub reason: String,
    pub confirmed: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPreviewInput {
    pub client_kind: super::MobileClient,
    pub draft_id: Option<Uuid>,
    pub expected_revision: Option<i64>,
    pub business_unit_id: Option<Uuid>,
    pub target_ids: Vec<Uuid>,
    pub message: String,
    pub between_targets_seconds: i64,
    #[serde(default)]
    pub selections: Vec<super::shared_send_v2::Selection>,
}
