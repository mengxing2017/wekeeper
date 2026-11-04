use crate::{
    business::{Page, Result, text},
    distribution::{
        domain::device_registry::{Command, Principal, Query, Scope},
        south::{adapter::postgres::device_registry::Repository, port::device_registry::Store},
    },
};
use serde_json::Value;
#[derive(Clone)]
pub struct Service<R = Repository> {
    pub store: R,
}
impl<R: Store> Service<R> {
    pub async fn execute(
        &self,
        principal: Principal,
        scope: Scope,
        key: &str,
        command: Command,
    ) -> Result<Value> {
        let key = text(key, 1, 128)?;
        self.store.execute(principal, scope, &key, command).await
    }
    pub async fn query(&self, p: Principal, s: Scope, q: Query, page: Page) -> Result<Value> {
        self.store.query(p, s, q, page).await
    }
}
impl crate::bootstrap::Application {
    pub fn device_registry(&self) -> Service {
        Service {
            store: Repository(self.pool.clone()),
        }
    }
}
