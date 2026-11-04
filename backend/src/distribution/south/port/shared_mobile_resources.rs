use crate::business::{Actor, Page, Result};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum Resource {
    Targets,
    Devices,
    PlatformDevices,
    Bindings,
    PlatformBindings,
    PlatformTargets,
}
pub trait Store: Clone + Send + Sync + 'static {
    fn target(
        &self,
        actor: &Actor,
        school: Uuid,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn list(
        &self,
        actor: &Actor,
        school: Option<Uuid>,
        resource: Resource,
        page: Page,
        search: String,
        device: Option<Uuid>,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn write(
        &self,
        actor: &Actor,
        school: Option<Uuid>,
        action: &str,
        id: Option<Uuid>,
        key: &str,
        input: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}
