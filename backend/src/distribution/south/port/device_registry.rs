use crate::{
    business::{Page, Result},
    distribution::domain::device_registry::{Command, Principal, Query, Scope},
};
use serde_json::Value;
pub trait Store: Clone + Send + Sync + 'static {
    fn execute(
        &self,
        principal: Principal,
        scope: Scope,
        key: &str,
        command: Command,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn query(
        &self,
        principal: Principal,
        scope: Scope,
        query: Query,
        page: Page,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}
