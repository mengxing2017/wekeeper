use crate::{business::Result, integrations::domain::directory::*};
use serde_json::Value;
use std::future::Future;
use uuid::Uuid;
pub trait DirectoryClient: Clone + Send + Sync + 'static {
    fn departments(
        &self,
    ) -> impl Future<Output = std::result::Result<Vec<Department>, ReadError>> + Send;
    fn members(
        &self,
        department: i64,
    ) -> impl Future<Output = std::result::Result<Vec<Member>, ReadError>> + Send;
    fn member(
        &self,
        userid: &str,
    ) -> impl Future<Output = std::result::Result<Member, ReadError>> + Send;
}
pub trait DirectoryStore: Clone + Send + Sync + 'static {
    fn configure(&self, corp: &str, scope: &str) -> impl Future<Output = Result<i64>> + Send;
    fn check_rotation(
        &self,
        corp: &str,
        scope: &str,
        expected_generation: i64,
    ) -> impl Future<Output = Result<()>> + Send;
    fn rotate(
        &self,
        actor: Uuid,
        corp: &str,
        scope: &str,
        expected_generation: i64,
    ) -> impl Future<Output = Result<()>> + Send;
    fn status(&self, corp: &str, scope: &str) -> impl Future<Output = Result<Value>> + Send;
    fn enqueue(
        &self,
        actor: Option<Uuid>,
        interval: i64,
        generation: i64,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn job(&self, id: Uuid) -> impl Future<Output = Result<Value>> + Send;
    fn claim(&self, generation: i64) -> impl Future<Output = Result<Option<Job>>> + Send;
    fn renew(&self, job: &Job) -> impl Future<Output = Result<()>> + Send;
    fn finish(
        &self,
        job: &Job,
        departments: &[Department],
        members: &[Member],
    ) -> impl Future<Output = Result<()>> + Send;
    fn fail(&self, job: &Job, code: &str) -> impl Future<Output = Result<()>> + Send;
    fn list(
        &self,
        q: &DirectoryQuery,
        departments: bool,
    ) -> impl Future<Output = Result<Vec<Value>>> + Send;
    fn bound_userids(&self) -> impl Future<Output = Result<Vec<String>>> + Send;
    fn member_id(&self, id: Uuid) -> impl Future<Output = Result<String>> + Send;
    fn account_userid(&self, id: Uuid) -> impl Future<Output = Result<String>> + Send;
    fn observe(
        &self,
        userid: &str,
        member: Option<&Member>,
        at: chrono::DateTime<chrono::Utc>,
        generation: i64,
    ) -> impl Future<Output = Result<()>> + Send;
    fn bind(
        &self,
        actor: Uuid,
        account: Uuid,
        member: Option<Uuid>,
        revision: i64,
        generation: Option<i64>,
    ) -> impl Future<Output = Result<()>> + Send;
}
