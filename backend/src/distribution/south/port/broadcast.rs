use crate::{
    business::{Actor, Page, Result},
    distribution::domain::broadcast::*,
};
use serde_json::Value;
use std::future::Future;
use uuid::Uuid;
#[derive(Clone)]
pub enum Command {
    Save {
        id: Option<Uuid>,
        input: DraftInput,
        corp: Option<String>,
        generation: Option<i64>,
        preview_key: Option<String>,
    },
    Action {
        id: Uuid,
        operation: String,
        key: String,
        input: Action,
    },
}
pub trait Store: Clone + Send + Sync + 'static {
    fn action_replay(
        &self,
        actor: &Actor,
        task: Uuid,
        operation: &str,
        key: &str,
        input: &Action,
    ) -> impl Future<Output = Result<bool>> + Send;
    fn channel_ready(&self) -> impl Future<Output = Result<bool>> + Send;
    fn preview_response(
        &self,
        actor: &Actor,
        key: &str,
        input_hash: &str,
    ) -> impl Future<Output = Result<Option<Value>>> + Send;
    fn evidence_query(
        &self,
        actor: &Actor,
        task: Uuid,
        batch: Uuid,
    ) -> impl Future<Output = Result<bool>> + Send;
    fn evidence_save(
        &self,
        actor: &Actor,
        task: Uuid,
        batch: Uuid,
        msgid: &str,
        summary: Value,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn evidence(
        &self,
        actor: &Actor,
        task: Uuid,
        batch: Uuid,
        id: Uuid,
    ) -> impl Future<Output = Result<String>> + Send;

    fn draft_request(
        &self,
        actor: &Actor,
        input: &DraftInput,
    ) -> impl Future<Output = Result<Option<Task>>> + Send;
    fn preview(
        &self,
        actor: &Actor,
        id: Uuid,
    ) -> impl Future<Output = Result<crate::distribution::domain::content::Preview>> + Send;
    fn authorize(&self, actor: Uuid, admin: bool) -> impl Future<Output = Result<()>> + Send;
    fn execute(&self, actor: &Actor, command: Command)
    -> impl Future<Output = Result<Task>> + Send;
    fn list(&self, page: Page) -> impl Future<Output = Result<Vec<Task>>> + Send;
    fn task(&self, id: Uuid) -> impl Future<Output = Result<Task>> + Send;
    fn rows(
        &self,
        id: Uuid,
        kind: &str,
        page: Page,
    ) -> impl Future<Output = Result<Vec<Value>>> + Send;
    fn work(&self, task: Option<Uuid>) -> impl Future<Output = Result<Vec<Work>>> + Send;
    fn claim(&self, work: &Work) -> impl Future<Output = Result<bool>> + Send;
    fn finish(&self, work: &Work, outcome: Outcome) -> impl Future<Output = Result<()>> + Send;
    fn observed(
        &self,
        work: &Work,
        results: Result<Results>,
    ) -> impl Future<Output = Result<()>> + Send;
}
pub trait Client: Clone + Send + Sync + 'static {
    fn generation(&self) -> impl Future<Output = Result<(String, i64)>> + Send;
    fn verify(&self, work: &Work) -> impl Future<Output = Result<()>> + Send;
    fn invoke(&self, work: &Work) -> impl Future<Output = Outcome> + Send;
    fn results(&self, work: &Work) -> impl Future<Output = Result<Results>> + Send;
    fn candidates(&self, start: i64, end: i64) -> impl Future<Output = Result<Vec<Value>>> + Send;
}
