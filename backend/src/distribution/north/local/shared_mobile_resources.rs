use crate::{
    business::{Actor, Page, Result, text},
    distribution::south::{
        adapter::postgres::shared_mobile_resources::Repository,
        port::shared_mobile_resources::{Resource, Store},
    },
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn target(&self, actor: &Actor, school: Uuid, id: Uuid) -> Result<Value> {
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
        if school.is_none() && !actor.platform_admin {
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
        if school.is_none() && !actor.platform_admin {
            return Err(crate::business::Error::Forbidden);
        }
        self.0
            .write(actor, school, action, id, &text(key, 1, 128)?, input)
            .await
    }
}
impl crate::bootstrap::Application {
    pub fn shared_mobile_resources(&self) -> Service {
        Service(Repository(self.pool.clone()))
    }
}
