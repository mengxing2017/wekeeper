use super::directory::{Job, ReadError, userid_key};
use crate::business::{Page, Result, require};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GroupMember {
    pub userid: String,
    pub r#type: i64,
    pub join_time: Option<i64>,
    pub join_scene: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Group {
    pub chat_id: String,
    pub name: Option<String>,
    pub owner: String,
    pub create_time: Option<i64>,
    pub member_version: Option<String>,
    pub member_list: Vec<GroupMember>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct GroupRef {
    pub chat_id: String,
    pub status: i64,
}
#[derive(Clone, Debug, Deserialize)]
pub struct GroupPage {
    pub group_chat_list: Vec<GroupRef>,
    #[serde(default)]
    pub next_cursor: String,
}
impl Group {
    pub fn member_count(&self) -> usize {
        self.member_list
            .iter()
            .map(|m| {
                (
                    m.r#type,
                    if m.r#type == 1 {
                        m.userid.to_ascii_lowercase()
                    } else {
                        m.userid.clone()
                    },
                )
            })
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    pub fn validate(&self, expected: &str) -> std::result::Result<(), ReadError> {
        if self.chat_id != expected || !opaque(expected) || userid_key(&self.owner).is_err() {
            return Err(ReadError::Invalid);
        }
        let mut members = std::collections::BTreeMap::new();
        for m in &self.member_list {
            let key = match m.r#type {
                1 => userid_key(&m.userid).map_err(|_| ReadError::Invalid)?,
                2 if opaque(&m.userid) => m.userid.clone(),
                _ => return Err(ReadError::Invalid),
            };
            if let Some(old) = members.insert((m.r#type, key), m)
                && old != m
            {
                return Err(ReadError::Invalid);
            }
        }
        Ok(())
    }
}
pub fn opaque(s: &str) -> bool {
    !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control)
}
pub fn edge(a: &str, b: &str) -> String {
    serde_json::json!([a, b]).to_string()
}
#[derive(Clone)]
pub struct AssetJob {
    pub fence: Job,
    pub dataset: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SyncInput {
    pub dataset: String,
}
impl SyncInput {
    pub fn validate(&self) -> Result<()> {
        require(self.dataset == "groups", "数据集必须为 groups")
    }
}
#[derive(Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssetQuery {
    #[serde(rename = "page[number]")]
    pub page: Option<i64>,
    #[serde(rename = "page[size]")]
    pub page_size: Option<i64>,
    pub search: Option<String>,
    pub member_id: Option<Uuid>,
    pub owner_member_id: Option<Uuid>,
    pub participant_member_id: Option<Uuid>,
    pub verification: Option<String>,
    pub associated: Option<bool>,
    pub dataset: Option<String>,
}
impl AssetQuery {
    pub fn pagination(&self) -> Page {
        Page {
            page: self.page,
            page_size: self.page_size,
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.pagination().bounds()?;
        require(
            self.search.as_ref().is_none_or(|v| v.len() <= 200),
            "搜索内容过长",
        )?;
        require(
            self.verification
                .as_ref()
                .is_none_or(|v| ["verified", "unverified", "unavailable"].contains(&v.as_str())),
            "核验状态无效",
        )?;
        require(
            self.dataset.as_ref().is_none_or(|v| v == "groups"),
            "数据集无效",
        )
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupBinding {
    pub directory_group_id: Uuid,
    pub expected_generation: i64,
    pub expected_checked_at: chrono::DateTime<chrono::Utc>,
    pub name: Option<String>,
}
pub fn value<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).expect("asset DTO serialization")
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerScopeDecision {
    pub run_id: Uuid,
    pub owner_userid: String,
    pub expected_generation: i64,
    pub reason: String,
}
