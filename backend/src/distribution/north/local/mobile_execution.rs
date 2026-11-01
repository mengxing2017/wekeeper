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
    pub async fn field_check_jobs(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        self.0.field_check_jobs(c).await
    }
    pub async fn field_check_device_write(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        id: Uuid,
        key: &str,
        action: &str,
        input: Value,
    ) -> Result<Value> {
        self.0
            .field_check_device_write(c, id, &text(key, 1, 128)?, action, input)
            .await
    }
    pub async fn opening_context(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Value> {
        self.0.opening_context(c).await
    }
    pub async fn environment(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: crate::distribution::domain::mobile_execution::onboarding::Environment,
    ) -> Result<Value> {
        input.validate()?;
        self.0.environment(c, &text(key, 1, 128)?, input).await
    }
    pub async fn capability_context(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        self.0.capability_context(c).await
    }
    pub async fn capability_decision(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: crate::distribution::domain::mobile_execution::shared_capability::DecisionInput,
    ) -> Result<Value> {
        input.validate()?;
        self.0
            .capability_decision(c, &text(key, 1, 128)?, input)
            .await
    }

    pub async fn validations(
        &self,
        c: &crate::distribution::domain::device_control::Credential,
    ) -> Result<Vec<Value>> {
        let _ = c;
        Err(crate::business::Error::Invalid(
            "旧手机业务协议已停止，请升级共享手机执行版本".into(),
        ))
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
        let _ = credential;
        Err(crate::business::Error::Invalid(
            "旧学校设备授权已停止，请升级共享手机执行版本".into(),
        ))
    }
    pub async fn consent(
        &self,
        credential: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: crate::distribution::domain::mobile_execution::ConsentInput,
    ) -> Result<Value> {
        let _ = (credential, key, input);
        Err(crate::business::Error::Invalid(
            "旧学校设备授权已停止，请升级共享手机执行版本".into(),
        ))
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
        if school.is_some() && matches!(action, "bind" | "update-binding" | "revoke-binding") {
            return Err(crate::business::Error::Forbidden);
        }
        if let Some(s) = school {
            actor.authorize(s, "admin")?;
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
