use crate::{
    business::{Actor, Page, Result, text},
    distribution::south::{
        adapter::postgres::mobile_execution::Repository,
        port::mobile_execution::{Resource, Store},
    },
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn validations(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        self.0.validations(c).await
    }
    pub async fn validation_result(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        id: Uuid,
        key: &str,
        input: crate::distribution::domain::mobile_execution::ValidationResultInput,
    ) -> Result<Value> {
        input.validate()?;
        self.0
            .validation_result(c, id, &text(key, 1, 128)?, input)
            .await
    }

    pub async fn consent_context(
        &self,
        credential: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        self.0.consent_context(credential).await
    }
    pub async fn consent(
        &self,
        credential: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: crate::distribution::domain::mobile_execution::ConsentInput,
    ) -> Result<Value> {
        if input.grant_revision < 1
            || input.expected_revision < 0
            || input.scope_digest.len() != 64
            || !input.scope_digest.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(crate::business::Error::Invalid("授权版本或摘要无效".into()));
        }
        self.0.consent(credential, &text(key, 1, 128)?, input).await
    }

    pub async fn target(&self, actor: &Actor, school: Uuid, id: Uuid) -> Result<Value> {
        actor.authorize(school, "operator")?;
        self.0.target(actor, school, id).await
    }

    pub async fn list(
        &self,
        actor: &Actor,
        school: Option<Uuid>,
        resource: Resource,
        page: Page,
        search: String,
        device: Option<Uuid>,
    ) -> Result<Vec<Value>> {
        if let Some(s) = school {
            actor.authorize(s, "operator")?;
        } else if !actor.platform_admin {
            return Err(crate::business::Error::Forbidden);
        }
        page.fetch_bounds()?;
        self.0
            .list(actor, school, resource, page, search, device)
            .await
    }
    pub async fn write(
        &self,
        actor: &Actor,
        school: Option<Uuid>,
        action: &str,
        id: Option<Uuid>,
        key: &str,
        input: Value,
    ) -> Result<Value> {
        if let Some(s) = school {
            actor.authorize(
                s,
                if matches!(action, "task-preview" | "cancel-task") {
                    "operator"
                } else {
                    "admin"
                },
            )?;
        } else if !actor.platform_admin {
            return Err(crate::business::Error::Forbidden);
        }
        self.0
            .write(actor, school, action, id, &text(key, 1, 128)?, input)
            .await
    }
}
impl crate::bootstrap::Application {
    pub fn mobile_execution(&self) -> Service {
        Service(Repository(self.pool.clone()))
    }
}
