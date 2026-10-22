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
        origin: String,
        corp: String,
        generation: i64,
    },
    Action {
        id: Uuid,
        operation: String,
        key: String,
        input: Action,
    },
}
pub trait Store: Clone + Send + Sync + 'static {
    fn authorize(
        &self,
        actor: Uuid,
        school: Uuid,
        admin: bool,
    ) -> impl Future<Output = Result<()>> + Send;
    fn execute(
        &self,
        actor: &Actor,
        school: Uuid,
        command: Command,
    ) -> impl Future<Output = Result<Task>> + Send;
    fn list(&self, school: Uuid, page: Page) -> impl Future<Output = Result<Vec<Task>>> + Send;
    fn task(&self, school: Uuid, id: Uuid) -> impl Future<Output = Result<Task>> + Send;
    fn rows(
        &self,
        school: Uuid,
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
