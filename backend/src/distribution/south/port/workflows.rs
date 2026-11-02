use crate::{
    business::{Actor, Page, Result},
    distribution::domain::workflows::Command,
};
use serde_json::Value;
use std::future::Future;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn service_identity(&self, id: Uuid) -> impl Future<Output = Result<()>> + Send;
    fn record_delivery(
        &self,
        id: Uuid,
        input: crate::distribution::domain::workflows::ChannelDelivery,
    ) -> impl Future<Output = Result<Uuid>> + Send;
    fn options(&self, a: &Actor, s: Option<Uuid>) -> impl Future<Output = Result<Value>> + Send;
    fn requests(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        page: Page,
    ) -> impl Future<Output = Result<Vec<Value>>> + Send;
    fn command(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        command: Command,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn channel(
        &self,
        a: &Actor,
        s: Uuid,
        configured: bool,
        enabled: bool,
        scope: &str,
    ) -> impl Future<Output = Result<Value>> + Send;
}
