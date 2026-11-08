use crate::{
    bootstrap::Application,
    business::{Actor, Result},
    distribution::south::adapter::postgres::mobile_workspaces::Repository,
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service(pub Repository);
impl Service {
    pub async fn list(&self, a: &Actor) -> Result<Vec<Value>> {
        self.0.list(a).await
    }
    pub async fn save(&self, a: &Actor, id: Uuid, name: &str) -> Result<Value> {
        a.platform()?;
        self.0.create(a, id, name).await
    }
    pub async fn members(&self, a: &Actor, id: Uuid) -> Result<Vec<Value>> {
        a.platform()?;
        self.0.members(a, id).await
    }
    pub async fn grant(&self, a: &Actor, id: Uuid, username: &str, role: &str) -> Result<Value> {
        a.platform()?;
        self.0.grant(a, id, username, role).await
    }
}
impl Application {
    pub fn mobile_workspaces(&self) -> Service {
        Service(Repository(self.pool.clone()))
    }
}
