use crate::business::{Result, require, text};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Confirmation {
    pub preview_id: Uuid,
    pub digest: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateInput {
    pub id: Option<Uuid>,
    #[serde(default)]
    pub revision: i64,
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub note: String,
    pub sample_post_id: Option<Uuid>,
    #[serde(default = "yes")]
    pub publish: bool,
}
fn yes() -> bool {
    true
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberPolicyInput {
    pub member_id: Uuid,
    pub generation: i64,
    pub revision: i64,
    pub enabled: bool,
    pub window_seconds: i32,
    pub max_calls: i32,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageInput {
    pub member_id: Uuid,
    pub account_id: Uuid,
    pub generation: i64,
    pub role: String,
    pub group_ids: Vec<Uuid>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestInput {
    pub reason: String,
    pub task_id: Option<Uuid>,
    pub batch_id: Option<Uuid>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestDecision {
    pub revision: i64,
    pub state: String,
    pub response: String,
    pub owner_id: Option<Uuid>,
}
#[derive(Clone)]
pub enum Command {
    TemplatePreview(TemplateInput),
    TemplateConfirm(Confirmation),
    PolicyPreview(MemberPolicyInput),
    PolicyConfirm(Confirmation),
    UsagePreview(UsageInput),
    UsageConfirm(Confirmation),
    Request { kind: String, input: RequestInput },
    Decision { id: Uuid, input: RequestDecision },
}
impl Command {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::TemplatePreview(v) => {
                text(&v.name, 1, 100)?;
                text(&v.note, 0, 1000)?;
                crate::distribution::domain::validate(&v.body)?;
                require(v.revision >= 0, "修订无效")
            }
            Self::PolicyPreview(v) => require(
                v.generation > 0
                    && v.revision >= 0
                    && (1..=2592000).contains(&v.window_seconds)
                    && (1..=100000).contains(&v.max_calls),
                "成员限制无效",
            ),
            Self::UsagePreview(v) => require(
                v.generation > 0
                    && matches!(v.role.as_str(), "operator" | "admin")
                    && !v.group_ids.is_empty()
                    && v.group_ids.len() <= 100
                    && v.group_ids
                        .iter()
                        .collect::<std::collections::BTreeSet<_>>()
                        .len()
                        == v.group_ids.len(),
                "请选择职责及 1–100 个不重复的群",
            ),
            Self::Request { kind, input } => {
                require(
                    matches!(kind.as_str(), "channel" | "broadcast_issue"),
                    "请求类型无效",
                )?;
                text(&input.reason, 1, 1000)?;
                require(
                    (kind != "broadcast_issue" || input.task_id.is_some())
                        && (kind != "channel"
                            || (input.task_id.is_none() && input.batch_id.is_none())),
                    "请选择需处理的任务",
                )
            }
            Self::Decision { input, .. } => {
                require(
                    matches!(
                        input.state.as_str(),
                        "investigating"
                            | "waiting_user"
                            | "approved"
                            | "declined"
                            | "resolved"
                            | "revoked"
                    ),
                    "处理状态无效",
                )?;
                text(&input.response, 1, 1000).map(|_| ())
            }
            _ => Ok(()),
        }
    }
}
#[derive(Clone)]
pub struct ChannelHealth {
    pub corp: Option<String>,
    pub generation: Option<i64>,
    pub scope: String,
    pub available: bool,
    pub error_code: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelControl {
    pub paused: bool,
    pub expected_revision: i64,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_control_matches_the_business_client_fixture_without_delivery_fields() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/product-workflows.json"
        ))
        .unwrap();
        let input: ChannelControl =
            serde_json::from_value(fixture["channel_control"].clone()).unwrap();
        assert_eq!(
            serde_json::to_value(input).unwrap(),
            fixture["channel_control"]
        );
        let mut obsolete = fixture["channel_control"].clone();
        obsolete["permission_review"] = serde_json::json!("not-a-business-input");
        assert!(serde_json::from_value::<ChannelControl>(obsolete).is_err());
    }
    #[test]
    fn no_technical_identity_in_public_business_inputs() {
        assert!(serde_json::from_value::<MemberPolicyInput>(serde_json::json!({"member_id":Uuid::nil(),"generation":1,"revision":0,"enabled":true,"window_seconds":3600,"max_calls":1,"resource_key":"wecom:forged"})).is_err());
        assert!(serde_json::from_value::<UsageInput>(serde_json::json!({"member_id":Uuid::nil(),"account_id":Uuid::nil(),"generation":1,"role":"admin","group_ids":[],"userid":"forged"})).is_err());
    }
}
