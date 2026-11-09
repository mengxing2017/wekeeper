//! Persistence port for independent API instances sharing the same durable state.
use crate::source_access::domain::{Result, Signed};
use serde_json::Value;
pub trait MachineStore {
    fn accept(
        &self,
        signed: Signed,
        body: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<(bool, Value)>> + Send;
}

/// Configuration use cases never depend on a SQL pool or an HTTP request.
pub trait RoutingStore: Clone + Send + Sync {
    fn execute(
        &self,
        actor: &crate::business::Actor,
        command: crate::source_access::domain::routing::Command,
    ) -> impl std::future::Future<Output = Result<serde_json::Value>> + Send;
    fn resolve(
        &self,
        receipt: uuid::Uuid,
    ) -> impl std::future::Future<Output = Result<crate::source_access::domain::routing::Resolution>>
    + Send;
}
