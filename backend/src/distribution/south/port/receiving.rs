use crate::{
    business::{Actor, Page, Result},
    distribution::domain::receiving::{
        Channel, Configuration, ConfigurationInput, TargetInput, TargetResource,
    },
};
use uuid::Uuid;

pub trait Store: Clone + Send + Sync + 'static {
    fn phone_progress(
        &self,
        actor: &Actor,
        receipt: Uuid,
        page: Page,
    ) -> impl Future<Output = Result<serde_json::Value>> + Send;
    fn stop_phone(
        &self,
        actor: &Actor,
        receipt: Uuid,
        target: Option<Uuid>,
    ) -> impl Future<Output = Result<serde_json::Value>> + Send;
    fn target(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
        input: TargetInput,
    ) -> impl std::future::Future<Output = Result<TargetResource>> + Send;
    fn resources(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
    ) -> impl std::future::Future<Output = Result<serde_json::Value>> + Send;
    fn save(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
        input: ConfigurationInput,
    ) -> impl std::future::Future<Output = Result<Configuration>> + Send;
    fn list(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        page: Page,
    ) -> impl std::future::Future<Output = Result<Vec<Configuration>>> + Send;
    fn get(
        &self,
        actor: &Actor,
        channel: Channel,
        unit: Uuid,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Configuration>> + Send;
}
