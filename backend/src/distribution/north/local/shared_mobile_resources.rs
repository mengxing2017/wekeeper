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
    pub async fn target(&self, actor: &Actor, id: Uuid) -> Result<Value> {
        self.0.target(actor, id).await
    }

    pub async fn list(
        &self,
        actor: &Actor,
        resource: Resource,
        page: Page,
        search: String,
        device: Option<Uuid>,
    ) -> Result<Vec<Value>> {
        actor.platform()?;
        page.fetch_bounds()?;
        self.0.list(actor, resource, page, search, device).await
    }
    pub async fn write(
        &self,
        actor: &Actor,
        action: &str,
        id: Option<Uuid>,
        key: &str,
        input: Value,
    ) -> Result<Value> {
        actor.platform()?;
        self.0
            .write(actor, action, id, &text(key, 1, 128)?, input)
            .await
    }
}
impl crate::bootstrap::Application {
    pub fn shared_mobile_resources(&self) -> Service {
        Service(Repository(self.pool.clone()))
    }
}
