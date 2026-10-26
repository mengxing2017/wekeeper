use crate::{
    business::{Page, Result, text},
    distribution::{
        domain::mobile_delivery::{Command, Principal, Query, Scope},
        south::{adapter::postgres::mobile_delivery::Repository, port::mobile_delivery::Store},
    },
};
use serde_json::Value;
#[derive(Clone)]
pub struct Service<R = Repository> {
    pub store: R,
    pub enabled: bool,
    pub origin: String,
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
        self.store
            .execute(principal, scope, &key, command, self.enabled, &self.origin)
            .await
    }
    pub async fn query(&self, p: Principal, s: Scope, q: Query, page: Page) -> Result<Value> {
        self.store.query(p, s, q, page).await
    }
    pub async fn authenticate(&self, token: &str) -> Result<Principal> {
        self.store.authenticate(token).await
    }
    pub async fn recover(&self) -> Result<()> {
        self.store.recover().await
    }
}
impl crate::bootstrap::Application {
    pub fn mobile_delivery(&self) -> Service {
        Service {
            store: Repository(self.pool.clone()),
            enabled: std::env::var("WEKEEPER_MOBILE_EXECUTION_ENABLED").is_ok_and(|s| s == "true"),
            origin: self.options.public_origin.clone(),
        }
    }
}

impl Service {
    pub async fn official_available(&self, school: uuid::Uuid) -> Result<bool> {
        self.store.official_available(school).await
    }
}
