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
