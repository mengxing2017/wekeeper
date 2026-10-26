use crate::{
    business::{Page, Result},
    distribution::domain::mobile_delivery::{Command, Principal, Query, Scope},
};
use serde_json::Value;
pub trait Store: Clone + Send + Sync + 'static {
    fn execute(
        &self,
        principal: Principal,
        scope: Scope,
        key: &str,
        command: Command,
        enabled: bool,
        origin: &str,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn query(
        &self,
        principal: Principal,
        scope: Scope,
        query: Query,
        page: Page,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn authenticate(
        &self,
        token: &str,
    ) -> impl std::future::Future<Output = Result<Principal>> + Send;
    fn recover(&self) -> impl std::future::Future<Output = Result<()>> + Send;
}
