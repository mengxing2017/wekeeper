use crate::business::{Result, require};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn userid_key(id: &str) -> Result<String> {
    require(
        !id.is_empty()
            && id.len() <= 64
            && id.is_ascii()
            && id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-.@".contains(&b)),
        "企业 UserID 格式无效",
    )?;
    Ok(id.to_ascii_lowercase())
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Department {
    pub id: i64,
    pub parentid: i64,
    pub name: String,
    #[serde(default)]
    pub order: i64,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Member {
    pub userid: String,
    pub name: String,
    pub status: Option<i32>,
    pub department: Vec<i64>,
    pub main_department: Option<i64>,
}
impl Member {
    pub fn validate(&self) -> Result<()> {
        userid_key(&self.userid)?;
        require(
            !self.name.trim().is_empty() && self.department.iter().all(|id| *id > 0),
            "企业成员数据不完整",
        )
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DirectoryMember {
    pub id: Uuid,
    pub userid: String,
    pub name: String,
    pub status: Option<i32>,
    pub visibility: String,
    pub verification: String,
    pub departments: Vec<Department>,
    pub checked_at: Option<String>,
    pub bound_account_id: Option<Uuid>,
    pub bound_account_name: Option<String>,
}
#[derive(Clone, Debug)]
pub enum ReadError {
    Transient,
    Rejected(i64),
    Invalid,
}
impl ReadError {
    pub fn code(&self) -> String {
        match self {
            Self::Transient => "network_or_rate_limit".into(),
            Self::Rejected(c) => format!("wecom_{c}"),
            Self::Invalid => "invalid_response".into(),
        }
    }
    pub fn unavailable_member(&self) -> bool {
        matches!(self, Self::Rejected(60111 | 60011 | 60121))
    }
}
#[derive(Clone)]
pub struct Job {
    pub id: Uuid,
    pub lease: Uuid,
    pub started_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectoryQuery {
    #[serde(rename = "page[number]")]
    pub page: Option<i64>,
    #[serde(rename = "page[size]")]
    pub page_size: Option<i64>,
    pub search: Option<String>,
    pub department_id: Option<i64>,
    pub status: Option<i32>,
    pub binding: Option<String>,
}
impl DirectoryQuery {
    pub fn pagination(&self) -> crate::business::Page {
        crate::business::Page {
            page: self.page,
            page_size: self.page_size,
            ..Default::default()
        }
    }
    pub fn validate(&self) -> Result<()> {
        self.pagination().bounds()?;
        require(
            self.search.as_ref().is_none_or(|s| s.len() <= 200)
                && self.department_id.is_none_or(|id| id > 0)
                && self
                    .binding
                    .as_deref()
                    .is_none_or(|b| ["bound", "unbound"].contains(&b)),
            "目录查询参数无效",
        )
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingInput {
    pub member_id: Uuid,
    pub revision: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn userid_is_case_insensitive_but_never_trimmed_or_guessed() {
        assert_eq!(userid_key("User.A-1@corp").unwrap(), "user.a-1@corp");
        for id in ["", " user", "成员", "a/b"] {
            assert!(userid_key(id).is_err());
        }
        assert!(userid_key(&"a".repeat(65)).is_err());
    }
}
