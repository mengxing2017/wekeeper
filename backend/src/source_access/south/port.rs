//! Persistence port for independent API instances sharing the same durable state.
use crate::source_access::domain::{Result, Signed};
use serde_json::Value;
pub trait MachineStore {
    fn record_failure(
        &self,
        key: &str,
        request_id: &str,
        code: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
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

pub trait ProjectionStore: Clone + Send + Sync {
    fn execute(
        &self,
        actor: &crate::business::Actor,
        command: crate::source_access::domain::projection::Command,
    ) -> impl std::future::Future<Output = crate::source_access::domain::Result<serde_json::Value>> + Send;
}

/// One bounded durable consumer tick. Scheduling belongs to the application runtime.
pub trait InboxWorker: Send + Sync {
    fn tick(&self) -> impl std::future::Future<Output = Result<()>> + Send;
}
