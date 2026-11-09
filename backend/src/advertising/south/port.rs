use super::super::domain::Command;
use crate::{business::Actor, source_access::domain::Result};
pub trait AdvertisingStore: Clone + Send + Sync {
    fn execute(
        &self,
        actor: &Actor,
        command: Command,
    ) -> impl std::future::Future<Output = Result<serde_json::Value>> + Send;
}
