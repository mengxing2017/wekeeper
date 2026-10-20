use crate::business::{Actor, Result};
use serde_json::Value;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn list(&self, a: &Actor) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn directory(&self) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn create(&self, name: &str) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn update(
        &self,
        a: &Actor,
        id: Uuid,
        name: &str,
        config: Value,
        revision: i64,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn bind(&self, a: &Actor, school: Uuid)
    -> impl std::future::Future<Output = Result<()>> + Send;
    fn members(&self, school: Uuid)
    -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn grant(
        &self,
        a: &Actor,
        school: Uuid,
        id: Uuid,
        role: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn resources(
        &self,
        school: Uuid,
        operator: bool,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn resource(
        &self,
        a: &Actor,
        school: Uuid,
        v: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn click(&self, school: Uuid, id: Uuid)
    -> impl std::future::Future<Output = Result<()>> + Send;
    fn groups(&self, school: Uuid) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn group(
        &self,
        a: &Actor,
        school: Uuid,
        v: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}
