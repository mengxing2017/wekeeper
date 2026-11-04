use crate::{
    business::{Actor, Page, Result},
    distribution::domain::workflows::Command,
};
use serde_json::Value;
use std::future::Future;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn claim_health_probe(&self, scope: &str) -> impl Future<Output = Result<bool>> + Send;
    fn record_health(
        &self,
        input: crate::distribution::domain::workflows::ChannelHealth,
    ) -> impl Future<Output = Result<()>> + Send;
    fn channel_control_read(
        &self,
        a: &Actor,
        scope: &str,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn channel_control(
        &self,
        a: &Actor,
        scope: &str,
        input: crate::distribution::domain::workflows::ChannelControl,
    ) -> impl Future<Output = Result<Value>> + Send;
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
        scope: &str,
    ) -> impl Future<Output = Result<Value>> + Send;
}
