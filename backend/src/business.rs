use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug)]
pub enum Error {
    Unauthorized,
    Forbidden,
    NotFound,
    Conflict,
    Invalid(String),
    Unavailable,
    Database(String),
}
pub type Result<T> = std::result::Result<T, Error>;

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}
pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::Invalid(message.into()))
    }
}
pub fn text(value: &str, min: usize, max: usize) -> Result<String> {
    let s = value.trim();
    require((min..=max).contains(&s.chars().count()), "文本长度不合法")?;
    Ok(s.into())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Grant {
    pub school_id: Uuid,
    pub role: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Actor {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub platform_admin: bool,
    pub bound_school_id: Option<Uuid>,
    pub grants: Vec<Grant>,
    #[serde(skip)]
    pub csrf: String,
    #[serde(skip)]
    pub session_hash: String,
    #[serde(skip)]
    pub request_id: String,
    #[serde(skip)]
    pub browser: bool,
    pub wecom_authenticated: bool,
}
impl Actor {
    pub fn can(&self, school: Uuid, role: &str) -> bool {
        self.platform_admin
            || self.grants.iter().any(|g| {
                g.school_id == school
                    && (role == "member"
                        || g.role == "admin"
                        || (role == "operator" && g.role == "operator"))
            })
    }
    pub fn authorize(&self, school: Uuid, role: &str) -> Result<()> {
        if self.can(school, role) {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
    pub fn platform(&self) -> Result<()> {
        if self.platform_admin {
            Ok(())
        } else {
            Err(Error::Forbidden)
        }
    }
}
#[derive(Deserialize, Default)]
pub struct Page {
    pub page: Option<i64>,
    pub page_size: Option<i64>,
    pub search: Option<String>,
    pub status: Option<String>,
    pub mine: Option<bool>,
    pub kind: Option<String>,
}
impl Page {
    pub fn bounds(&self) -> Result<(i64, i64)> {
        let size = self.page_size.unwrap_or(30);
        let page = self.page.unwrap_or(1);
        require(
            (1..=100).contains(&size) && (1..=100_000).contains(&page),
            "分页参数不合法",
        )?;
        Ok((size, (page - 1) * size))
    }
}
#[derive(Deserialize)]
pub struct Revision {
    pub revision: i64,
}
#[derive(Deserialize)]
pub struct Decision {
    pub revision: i64,
    pub decision: String,
    #[serde(default)]
    pub reason: String,
}
#[derive(Deserialize)]
pub struct Named {
    pub name: String,
}
pub fn uuid(value: &Value, key: &str) -> Result<Uuid> {
    value[key]
        .as_str()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| Error::Invalid(format!("缺少有效的 {key}")))
}
pub fn hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
pub fn secret() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}
