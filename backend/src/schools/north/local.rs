use super::super::south::adapter::postgres::Repository;
use crate::business::{Actor, Result, require, text};
use crate::schools::south::port::Store;
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn create(&self, a: &Actor, name: &str) -> Result<Value> {
        a.platform()?;
        self.0.create(&text(name, 1, 100)?).await
    }
    pub async fn update(&self, a: &Actor, id: Uuid, v: Value) -> Result<Value> {
        a.authorize(id, "admin")?;
        let config = v
            .get("config")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        require(
            config.is_object() && config.to_string().len() < 16384,
            "学校配置无效",
        )?;
        for key in ["post_audit_mode", "comment_audit_mode"] {
            if let Some(v) = config.get(key) {
                require(v == "auto" || v == "manual", "审核模式只能为 auto/manual")?;
            }
        }
        self.0
            .update(
                a,
                id,
                &text(v["name"].as_str().unwrap_or(""), 1, 100)?,
                config,
                v["revision"].as_i64().unwrap_or(0),
            )
            .await
    }
    pub async fn grant(&self, a: &Actor, school: Uuid, id: Uuid, role: &str) -> Result<()> {
        a.platform()?;
        require(
            ["member", "operator", "admin", "revoked"].contains(&role),
            "角色无效",
        )?;
        self.0.grant(a, school, id, role).await
    }
    pub async fn resources(&self, a: &Actor, school: Uuid) -> Result<Vec<Value>> {
        a.authorize(school, "member")?;
        self.0.resources(school, a.can(school, "operator")).await
    }
    pub async fn resource(&self, a: &Actor, school: Uuid, v: Value) -> Result<Value> {
        if school.is_nil() {
            a.platform()?;
        } else {
            a.authorize(school, "admin")?;
        }
        require(
            [
                "banner",
                "qrcode",
                "service",
                "legal",
                "tag",
                "blocked_word",
            ]
            .contains(&v["kind"].as_str().unwrap_or("")),
            "资源类型无效",
        )?;
        text(v["title"].as_str().unwrap_or(""), 1, 200)?;
        let u = v["url"].as_str().unwrap_or("");
        require(
            u.is_empty()
                || u.starts_with("https://")
                || (u.starts_with('/') && !u.starts_with("//") && !u.contains('\\')),
            "链接必须为 HTTPS 或本站路径",
        )?;
        for k in ["starts_at", "ends_at"] {
            if let Some(s) = v[k].as_str() {
                require(
                    chrono::DateTime::parse_from_rfc3339(s).is_ok(),
                    "日期格式错误",
                )?
            }
        }
        self.0.resource(a, school, v).await
    }
    pub async fn group(&self, a: &Actor, school: Uuid, v: Value) -> Result<Value> {
        if school.is_nil() {
            a.platform()?;
        } else {
            a.authorize(school, "admin")?;
        }
        for k in ["corp_id", "chat_id", "name"] {
            text(v[k].as_str().unwrap_or(""), 1, 200)?;
        }
        self.0.group(a, school, v).await
    }
}

impl<R: Store> Service<R> {
    pub async fn list(&self, a: &Actor) -> Result<Vec<Value>> {
        self.0.list(a).await
    }
    pub async fn directory(&self) -> Result<Vec<Value>> {
        self.0.directory().await
    }
    pub async fn bind(&self, a: &Actor, s: Uuid) -> Result<()> {
        self.0.bind(a, s).await
    }
    pub async fn members(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "admin")?;
        self.0.members(s).await
    }
    pub async fn click(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<()> {
        a.authorize(s, "member")?;
        self.0.click(s, id).await
    }
    pub async fn groups(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        self.0.groups(s).await
    }
}

impl<R: Store> Service<R> {
    pub async fn global_resources(&self, a: &Actor) -> Result<Vec<Value>> {
        a.platform()?;
        self.0.resources(Uuid::nil(), true).await
    }
}
