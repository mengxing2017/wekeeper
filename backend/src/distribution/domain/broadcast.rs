use crate::business::{Error, Result, require};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContentSource {
    #[default]
    Posts,
    Text,
}
impl ContentSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Posts => "posts",
            Self::Text => "text",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DraftInput {
    pub text_content: Option<String>,
    pub template_version_id: Option<Uuid>,
    #[serde(default)]
    pub content_source: ContentSource,
    #[serde(default)]
    pub post_ids: Vec<Uuid>,
    pub group_ids: Vec<Uuid>,
    pub revision: Option<i64>,
    pub source_task_id: Option<Uuid>,
    pub source_draft_id: Option<Uuid>,
    #[serde(default)]
    pub reason: String,
}
impl DraftInput {
    pub fn validate(&self) -> Result<()> {
        match self.content_source {
            ContentSource::Posts => {
                require(
                    self.template_version_id.is_some() && (1..=20).contains(&self.post_ids.len()),
                    "请选择已发布模板及 1–20 条内容",
                )?;
                require(
                    self.text_content.as_deref().is_none_or(str::is_empty),
                    "社区内容模式不能填写直接文本",
                )?;
            }
            ContentSource::Text => {
                require(
                    self.template_version_id.is_none() && self.post_ids.is_empty(),
                    "直接文本不能关联帖子或模板",
                )?;
                normalize_text(self.text_content.as_deref().unwrap_or_default())?;
                require(
                    self.source_draft_id.is_none(),
                    "历史分享草稿须按社区内容迁移",
                )?;
            }
        }
        require(
            (1..=10000).contains(&self.group_ids.len()),
            "请选择 1–10000 个客户群",
        )?;
        for ids in [&self.post_ids, &self.group_ids] {
            require(
                ids.iter().collect::<BTreeSet<_>>().len() == ids.len(),
                "目标或内容不可重复",
            )?;
        }
        require(self.reason.chars().count() <= 1000, "原因过长")?;
        if self.source_task_id.is_some() {
            require(!self.reason.trim().is_empty(), "再次发送必须填写原因")?;
        }
        Ok(())
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Action {
    pub revision: Option<i64>,
    pub batch_id: Option<Uuid>,
    pub decision: Option<String>,
    pub msgid: Option<String>,
    #[serde(default)]
    pub reason: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub school_id: Uuid,
    pub creator_id: Uuid,
    pub template_version_id: Option<Uuid>,
    #[serde(default)]
    pub content_source: ContentSource,
    pub content: String,
    pub revision: i64,
    pub state: String,
    pub stopped: bool,
    pub created_at: String,
    pub total: i64,
    pub sent: i64,
    pub rejected: i64,
    pub uncertain: i64,
    pub pending: i64,
    pub batch_count: i64,
    pub post_ids: Vec<Uuid>,
    pub source_task_id: Option<Uuid>,
    pub source_draft_id: Option<Uuid>,
    pub resend_reason: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Target {
    pub id: Uuid,
    pub group_id: Uuid,
    pub chat_id: String,
    pub group_name: String,
    pub batch_id: Uuid,
    pub sender: String,
    pub creation_state: String,
    pub msgid: Option<String>,
    pub rejected: bool,
    pub official_status: Option<i32>,
    pub send_time: Option<i64>,
    pub member_status: Option<i32>,
    pub last_synced_at: Option<String>,
    pub sync_error: Option<String>,
    pub error_code: Option<String>,
    pub stop_state: String,
    pub reconciliation: Option<String>,
    pub operation_unknown: bool,
}
#[derive(Clone, Deserialize)]
pub struct Work {
    pub id: Uuid,
    pub school_id: Uuid,
    pub task_id: Uuid,
    pub sender: String,
    pub sender_account: Uuid,
    pub content: String,
    pub corp_id: String,
    pub generation: i64,
    pub chats: Vec<String>,
    pub msgid: Option<String>,
    pub operation: String,
    pub operation_id: Option<Uuid>,
}
#[derive(Clone, Debug)]
pub enum Outcome {
    Created { msgid: String, failed: Vec<String> },
    Done,
    Rejected(String),
    Unknown,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    pub chat_id: String,
    pub status: i32,
    pub send_time: Option<i64>,
}
#[derive(Clone, Debug)]
pub struct Results {
    pub member_status: Option<i32>,
    pub groups: Vec<Observation>,
}

pub fn normalize_text(content: &str) -> Result<String> {
    let content = content.replace("\r\n", "\n").replace('\r', "\n");
    validate_content(&content)?;
    require(
        !content
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t'),
        "正文包含不支持的控制字符",
    )?;
    Ok(content)
}

pub fn validate_content(content: &str) -> Result<()> {
    require(
        !content.trim().is_empty() && content.len() <= 4000,
        "正文必须为 1–4000 UTF-8 字节",
    )
}
pub fn chunks<T>(targets: Vec<(String, T)>) -> Vec<(String, Vec<T>)> {
    let mut owners = BTreeMap::<String, Vec<T>>::new();
    for (owner, target) in targets {
        owners.entry(owner).or_default().push(target);
    }
    let mut out = Vec::new();
    for (owner, targets) in owners {
        let mut iter = targets.into_iter().peekable();
        while iter.peek().is_some() {
            out.push((owner.clone(), iter.by_ref().take(2000).collect()));
        }
    }
    out
}
pub fn decode<T: serde::de::DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|_| Error::Database("invalid broadcast data".into()))
}
pub fn created_response(v: Value, chats: &[String]) -> Outcome {
    let Some(code) = v["errcode"].as_i64() else {
        return Outcome::Unknown;
    };
    if code != 0 {
        return Outcome::Rejected(code.to_string());
    }
    let Some(msgid) = v["msgid"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 512)
    else {
        return Outcome::Unknown;
    };
    let failed: Vec<String> = match v.get("fail_list") {
        None => Vec::new(),
        Some(v) => match serde_json::from_value(v.clone()) {
            Ok(v) => v,
            Err(_) => return Outcome::Unknown,
        },
    };
    if failed.iter().any(|id| !chats.contains(id)) {
        return Outcome::Unknown;
    }
    Outcome::Created {
        msgid: msgid.into(),
        failed,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_text_validation_and_legacy_input() {
        assert_eq!(
            normalize_text("通知\r\n第二行\r第三行\t链接").unwrap(),
            "通知\n第二行\n第三行\t链接"
        );
        assert!(normalize_text(&"😀".repeat(1000)).is_ok());
        assert!(normalize_text(&format!("{}a", "😀".repeat(1000))).is_err());
        for bad in [" \t\n", "hello\0", "hello\u{85}"] {
            assert!(normalize_text(bad).is_err());
        }
        let id = Uuid::new_v4();
        let legacy = serde_json::json!({"template_version_id":id,"post_ids":[id],"group_ids":[id]});
        let input: DraftInput = serde_json::from_value(legacy).unwrap();
        assert_eq!(input.content_source, ContentSource::Posts);
        assert!(input.validate().is_ok());
        let direct =
            serde_json::json!({"content_source":"text","text_content":"notice","group_ids":[id]});
        let mut input: DraftInput = serde_json::from_value(direct.clone()).unwrap();
        assert!(input.validate().is_ok());
        input.template_version_id = Some(id);
        assert!(input.validate().is_err());
        input.template_version_id = None;
        input.post_ids = vec![id];
        assert!(input.validate().is_err());
        let mut unknown = direct;
        unknown["content_source"] = serde_json::json!("other");
        assert!(serde_json::from_value::<DraftInput>(unknown).is_err());
    }

    #[test]
    fn byte_limit_and_owner_partition() {
        assert!(validate_content(&"中".repeat(1333)).is_ok());
        assert!(validate_content(&"中".repeat(1334)).is_err());
        let parts = chunks(
            (0..2001)
                .map(|i| ("owner".into(), i))
                .chain([("other".into(), 9)])
                .collect(),
        );
        assert_eq!(
            parts.iter().map(|(_, v)| v.len()).collect::<Vec<_>>(),
            [1, 2000, 1]
        );
    }
    #[test]
    fn malformed_success_is_unknown() {
        assert!(matches!(
            created_response(serde_json::json!({"errcode":0}), &[]),
            Outcome::Unknown
        ));
        assert!(matches!(
            created_response(serde_json::json!({"msgid":"x"}), &[]),
            Outcome::Unknown
        ));
        assert!(matches!(
            created_response(
                serde_json::json!({"errcode":0,"msgid":"x","fail_list":["a"]}),
                &["a".into()]
            ),
            Outcome::Created { .. }
        ));
    }
}
