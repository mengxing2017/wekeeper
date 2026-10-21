use crate::integrations::domain::{assets::*, directory::ReadError};
use std::future::Future;
pub trait AssetClient: Clone + Send + Sync + 'static {
    fn groups(
        &self,
        owners: &[String],
        cursor: &str,
    ) -> impl Future<Output = Result<GroupPage, ReadError>> + Send;
    fn group(&self, id: &str) -> impl Future<Output = Result<Group, ReadError>> + Send;
}

use crate::business::Actor;
use serde_json::Value;
use uuid::Uuid;
/// Persistence boundary: implementations must fence staging and publication by generation and lease.
pub trait AssetStore: Clone + Send + Sync + 'static {
    fn enqueue(
        &self,
        actor: Option<Uuid>,
        g: i64,
        dataset: &str,
        interval: i64,
    ) -> impl Future<Output = crate::business::Result<Vec<Value>>> + Send;
    fn status(&self) -> impl Future<Output = crate::business::Result<Value>> + Send;
    fn claim(
        &self,
        g: i64,
    ) -> impl Future<Output = crate::business::Result<Option<AssetJob>>> + Send;
    fn renew(&self, j: &AssetJob) -> impl Future<Output = crate::business::Result<Value>> + Send;
    fn stage(
        &self,
        j: &AssetJob,
        kind: &str,
        key: &str,
        data: Value,
    ) -> impl Future<Output = crate::business::Result<()>> + Send;
    fn page(&self, j: &AssetJob) -> impl Future<Output = crate::business::Result<()>> + Send;
    fn items(
        &self,
        j: &AssetJob,
        kind: &str,
        after: &str,
    ) -> impl Future<Output = crate::business::Result<Vec<(String, Value)>>> + Send;
    fn scope(
        &self,
        j: &AssetJob,
        scope: &[String],
    ) -> impl Future<Output = crate::business::Result<()>> + Send;
    fn owners(&self, g: i64) -> impl Future<Output = crate::business::Result<Vec<String>>> + Send;
    fn configured_groups(
        &self,
    ) -> impl Future<Output = crate::business::Result<Vec<String>>> + Send;
    fn contains(
        &self,
        j: &AssetJob,
        kind: &str,
        key: &str,
    ) -> impl Future<Output = crate::business::Result<bool>> + Send;
    fn complete(&self, j: &AssetJob) -> impl Future<Output = crate::business::Result<()>> + Send;
    fn finish(&self, j: &AssetJob) -> impl Future<Output = crate::business::Result<()>> + Send;
    fn fail(
        &self,
        j: &AssetJob,
        code: &str,
    ) -> impl Future<Output = crate::business::Result<()>> + Send;
    fn list(
        &self,
        kind: &str,
        q: &AssetQuery,
        parent: Option<Uuid>,
        id: Option<Uuid>,
    ) -> impl Future<Output = crate::business::Result<Vec<Value>>> + Send;
    fn candidates(
        &self,
        a: &Actor,
        school: Uuid,
        q: &AssetQuery,
        g: i64,
    ) -> impl Future<Output = crate::business::Result<Vec<Value>>> + Send;
    fn bind_group(
        &self,
        a: &Actor,
        school: Uuid,
        input: &GroupBinding,
        group: &Group,
    ) -> impl Future<Output = crate::business::Result<Value>> + Send;
    fn candidate(
        &self,
        a: &Actor,
        school: Uuid,
        id: Uuid,
        g: i64,
    ) -> impl Future<Output = crate::business::Result<Value>> + Send;
    fn observe(
        &self,
        generation_id: i64,
        group: &Group,
        at: chrono::DateTime<chrono::Utc>,
    ) -> impl Future<Output = crate::business::Result<Uuid>> + Send;
    fn failed_owner(
        &self,
        j: &AssetJob,
        owner: &str,
    ) -> impl Future<Output = crate::business::Result<()>> + Send;
    fn confirm_scope(
        &self,
        a: &Actor,
        input: &OwnerScopeDecision,
    ) -> impl Future<Output = crate::business::Result<()>> + Send;
}
