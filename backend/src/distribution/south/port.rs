use crate::business::{Actor, Result};
use serde_json::Value;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn claim(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn templates(&self, s: Uuid) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn template(
        &self,
        s: Uuid,
        name: &str,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn version(
        &self,
        s: Uuid,
        id: Uuid,
        body: &str,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn publish(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        revision: i64,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn drafts(&self, s: Uuid) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn draft(
        &self,
        a: &Actor,
        s: Uuid,
        group: Uuid,
        version: Uuid,
        ids: Vec<Uuid>,
        origin: &str,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn context(
        &self,
        s: Uuid,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<(String, String)>> + Send;
    #[allow(clippy::too_many_arguments)]
    fn attempt(
        &self,
        a: &Actor,
        s: Uuid,
        draft: Uuid,
        key: &str,
        chat: &str,
        resend: Option<Uuid>,
        reason: &str,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn attempts(&self, s: Uuid) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn result(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn reconcile(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}
