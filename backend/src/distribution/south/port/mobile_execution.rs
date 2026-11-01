use crate::business::{Actor, Page, Result};
use serde_json::Value;
use uuid::Uuid;

#[derive(Clone, Copy)]
pub enum Resource {
    Tasks,
    Flows,
    Validations,
    Releases,
    Assignments,
    Targets,
    Devices,
    PlatformDevices,
    Bindings,
    Grants,
}
pub trait Store: Clone + Send + Sync + 'static {
    fn validations(
        &self,
        credential: &crate::distribution::domain::device_control::Credential,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn validation_result(
        &self,
        credential: &crate::distribution::domain::device_control::Credential,
        id: Uuid,
        key: &str,
        input: crate::distribution::domain::mobile_execution::ValidationResultInput,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;

    fn consent_context(
        &self,
        credential: &crate::distribution::domain::device_control::Credential,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn consent(
        &self,
        credential: &crate::distribution::domain::device_control::Credential,
        key: &str,
        input: crate::distribution::domain::mobile_execution::ConsentInput,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;

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
