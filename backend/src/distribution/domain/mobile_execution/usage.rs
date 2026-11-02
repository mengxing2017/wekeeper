//! Business-only commands. Technical delivery identifiers are never accepted from a business form.
use super::shared_send_v2::{Selection, require};
use crate::business::{Result, text};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UsageTarget {
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
            text(&t.search_text, 1, 256)?;
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
