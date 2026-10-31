use super::super::{domain, south::adapter::postgres::Repository};
use crate::business::{Actor, Result, require, text};
use crate::distribution::south::port::Store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
#[derive(Deserialize)]
pub struct DraftInput {
    pub group_id: Uuid,
    pub template_version_id: Uuid,
    pub post_ids: Vec<Uuid>,
}
#[derive(Deserialize)]
pub struct AttemptInput {
    pub current_chat_id: String,
    pub resend_of: Option<Uuid>,
    #[serde(default)]
    pub reason: String,
}
impl<R: Store> Service<R> {
    pub async fn claim(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<()> {
        a.authorize(s, "operator")?;
        if !a.wecom_authenticated {
            return Err(crate::business::Error::Forbidden);
        }
        self.0.claim(a, s, id).await
    }

    pub async fn template(&self, a: &Actor, s: Uuid, name: &str) -> Result<Value> {
        a.authorize(s, "operator")?;
        self.0.template(s, &text(name, 1, 100)?).await
    }
    pub async fn version(&self, a: &Actor, s: Uuid, id: Uuid, body: &str) -> Result<Value> {
        a.authorize(s, "operator")?;
        domain::validate(body)?;
        self.0.version(s, id, body).await
    }
    pub async fn draft(&self, a: &Actor, s: Uuid, v: DraftInput, origin: &str) -> Result<Value> {
        a.authorize(s, "operator")?;
        require(
            !v.post_ids.is_empty() && v.post_ids.len() <= 20,
            "请选择 1–20 条内容",
        )?;
        let mut ids = v.post_ids.clone();
        ids.sort();
        ids.dedup();
        require(ids.len() == v.post_ids.len(), "内容不可重复")?;
        self.0
            .draft(a, s, v.group_id, v.template_version_id, v.post_ids, origin)
            .await
    }
    pub async fn result(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        a.authorize(s, "operator")?;
        require(
            [
                "client_reported_success",
                "client_reported_failure",
                "cancelled",
                "unknown",
            ]
            .contains(&v["state"].as_str().unwrap_or("")),
            "发送结果无效",
        )?;
        require(v.to_string().len() <= 2000, "结果过长")?;
        self.0.result(a, s, id, v).await
    }
    pub async fn reconcile(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        a.authorize(s, "admin")?;
        require(
            ["operator_confirmed", "operator_rejected"]
                .contains(&v["decision"].as_str().unwrap_or("")),
            "核对结论无效",
        )?;
        text(v["reason"].as_str().unwrap_or(""), 1, 1000)?;
        self.0.reconcile(a, s, id, v).await
    }
}

impl<R: Store> Service<R> {
    pub async fn get_draft(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<Value> {
        a.authorize(s, "operator")?;
        self.0.get_draft(s, id).await
    }
    pub async fn related(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        page: crate::business::Page,
    ) -> Result<crate::distribution::domain::RelatedHistory> {
        a.authorize(s, "operator")?;
        let (limit, offset) = page.fetch_bounds()?;
        self.0.related(s, id, limit, offset).await
    }
    pub async fn templates(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        self.0.templates(s).await
    }
    pub async fn publish(&self, a: &Actor, s: Uuid, id: Uuid, rev: i64) -> Result<Value> {
        a.authorize(s, "operator")?;
        self.0.publish(a, s, id, rev).await
    }
    pub async fn drafts(
        &self,
        a: &Actor,
        s: Uuid,
        page: crate::business::Page,
    ) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        let (limit, offset) = page.fetch_bounds()?;
        self.0.drafts(s, limit, offset).await
    }
    pub async fn attempts(
        &self,
        a: &Actor,
        s: Uuid,
        page: crate::business::Page,
    ) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        let (limit, offset) = page.fetch_bounds()?;
        self.0.attempts(s, limit, offset).await
    }
    pub async fn prepare(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        key: &str,
        v: AttemptInput,
        integration: &crate::integrations::north::local::Service,
    ) -> Result<Value> {
        a.authorize(s, "operator")?;
        if !a.wecom_authenticated {
            return Err(crate::business::Error::Forbidden);
        }
        let key = text(key, 1, 128)?;
        text(&v.reason, 0, 1000)?;
        if v.resend_of.is_some() {
            a.authorize(s, "admin")?;
        }
        let (corp, chat) = self.0.context(s, id).await?;
        if chat != v.current_chat_id {
            return Err(crate::business::Error::Forbidden);
        }
        integration.verify_group(a, &corp, &chat).await?;
        self.0
            .attempt(a, s, id, &key, &chat, v.resend_of, &v.reason)
            .await
    }
}

pub mod mobile_delivery;

pub mod device_control;
