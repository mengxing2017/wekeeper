//! Business-only commands. Technical delivery identifiers are never accepted from a business form.
use super::shared_send_v2::{Selection, require};
use crate::business::{Result, text};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
/// Server-resolved location shown in the opening summary, never a client input.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageLocator {
    #[serde(default)]
    pub target: serde_json::Value,
    pub device_id: Uuid,
    pub device_name: String,
    pub target_id: Option<Uuid>,
    pub target_index: usize,
    pub target_name: String,
    pub search_text: String,
    pub search_source: String,
    pub binding_locator_revision: Option<i64>,
    pub target_locator_revision: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageTarget {
    #[serde(default)]
    pub exact_name: Option<String>,
    pub id: Option<Uuid>,
    pub name: String,
    pub search_text: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageInput {
    pub device_ids: Vec<Uuid>,
    pub targets: Vec<UsageTarget>,
    pub max_calls: i64,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub auto_targets: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub renewal_of: Option<Uuid>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_revision: Option<i64>,
}
impl UsageInput {
    pub fn validate(&self) -> Result<()> {
        require(
            !self.device_ids.is_empty()
                && !self.targets.is_empty()
                && self.device_ids.len() * self.targets.len() <= 100,
            "一次最多选择100个手机与群绑定",
        )?;
        require(
            self.device_ids
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                == self.device_ids.len(),
            "手机不能重复",
        )?;
        let ids: Vec<_> = self.targets.iter().filter_map(|t| t.id).collect();
        require(
            ids.iter().collect::<std::collections::HashSet<_>>().len() == ids.len(),
            "目标群不能重复",
        )?;
        require(
            self.max_calls > 0
                && self.max_calls <= 100000
                && self.expires_at > chrono::Utc::now()
                && self.expires_at < chrono::Utc::now() + chrono::Duration::days(366),
            "请填写有限额度和有效期",
        )?;
        for t in &self.targets {
            text(&t.name, 1, 256)?;
            super::validate_text(&t.search_text, 1, 256, 1024)?;
            if let Some(name) = &t.exact_name {
                super::validate_text(name, 1, 256, 1024)?;
            }
            require(
                t.id.is_some() || t.exact_name.is_some(),
                "target_identity_required",
            )?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageConfirm {
    pub preview_id: Uuid,
    pub digest: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageMutation {
    pub expected_revision: i64,
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageTaskInput {
    pub draft_id: Option<Uuid>,
    pub expected_revision: Option<i64>,
    pub target_ids: Vec<Uuid>,
    pub message: String,
    pub between_targets_seconds: i64,
    #[serde(default)]
    pub selections: Vec<Selection>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", content = "input", rename_all = "snake_case")]
pub enum UsageCommand {
    Preview(UsageInput),
    Confirm(UsageConfirm),
    Mutate {
        id: Uuid,
        operation: String,
        mutation: UsageMutation,
    },
    TaskPreview(UsageTaskInput),
    TaskConfirm(super::shared_send_v2::Confirmation),
    IssueUpdate {
        id: Uuid,
        issue_id: Uuid,
        input: UsageIssueUpdate,
    },
    Issue {
        id: Uuid,
        description: String,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageIssueUpdate {
    pub expected_revision: i64,
    pub state: String,
    pub response: String,
}

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
            inspection_seconds: 3600,
            retry_seconds: 300,
            retry_limit: 3,
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
            Some("preview_only") => "preview_only",
            Some("draft_only") => "draft_only",
            _ => "disabled",
        };
        let (owner, message) = match platform {
            "enabled" => ("school", "平台已开放发送；手机领取任务并完成本地准备后执行"),
            "default" => ("school", "使用系统默认发送设置，可正常提交任务"),
            "preview_only" => ("platform", "任务可以提交，实际执行等待平台恢复发送"),
            "draft_only" => ("platform", "任务可以提交，实际执行等待平台恢复发送"),
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

#[cfg(test)]
mod policy_tests {
    use super::*;
    #[test]
    fn business_permission_depends_only_on_the_platform_scheme() {
        for mode in [
            None,
            Some("disabled"),
            Some("draft_only"),
            Some("preview_only"),
        ] {
            let s = SendingStatus::project(mode);
            assert!(s.can_confirm);
            assert_eq!(s.owner, if mode.is_none() { "school" } else { "platform" });
        }
        assert_eq!(SendingStatus::project(Some("enabled")).platform, "enabled");
        assert!(SendingStatus::project(Some("enabled")).can_confirm);
        let wire = serde_json::to_value(SendingStatus::project(Some("enabled"))).unwrap();
        assert!(wire.get("delivery").is_none());
        assert_eq!(wire["can_confirm"], true);
    }
    #[test]
    fn business_form_cannot_select_technical_stages_or_skip_confirmation() {
        let limits = serde_json::json!({"interval_seconds":30,"window_seconds":3600,"max_calls":30,"daily_max_calls":150});
        let mut body = serde_json::json!({"expected_revision":0,"enabled":false,"confirmed":true,"timezone":"Asia/Shanghai","allowed_windows":[{"weekdays":[1,2,3,4,5],"start_minute":540,"end_minute":1080}],"limits":{"global":limits,"school":limits,"device":limits,"target":limits},"reason":"业务设置"});
        let p = serde_json::from_value::<BusinessSchemeInput>(body.clone())
            .unwrap()
            .policy()
            .unwrap();
        assert_eq!(p.mode, "disabled");
        body["confirmed"] = serde_json::json!(false);
        assert!(
            serde_json::from_value::<BusinessSchemeInput>(body.clone())
                .unwrap()
                .policy()
                .is_err()
        );
        body["allowed_stages"] = serde_json::json!(["test_send"]);
        assert!(serde_json::from_value::<BusinessSchemeInput>(body).is_err());
    }
}
