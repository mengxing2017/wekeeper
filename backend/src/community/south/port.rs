use crate::business::{Actor, Decision, Page, Result};
use crate::community::domain::{CommentInput, PostInput};
use serde_json::Value;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn validate_media(
        &self,
        s: Uuid,
        images: &[String],
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn check_words(
        &self,
        s: Uuid,
        content: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn posts(
        &self,
        a: &Actor,
        s: Uuid,
        p: Page,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn detail(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn create(
        &self,
        a: &Actor,
        s: Uuid,
        v: PostInput,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn update(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: PostInput,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn visibility(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn moderate(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Decision,
        comment: bool,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn react(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        kind: &str,
        add: bool,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn reactions(
        &self,
        a: &Actor,
        s: Uuid,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn comments(
        &self,
        a: &Actor,
        s: Uuid,
        id: Option<Uuid>,
        p: Page,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn comment(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: CommentInput,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn delete_comment(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        revision: i64,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn follow(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        add: bool,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn follows(
        &self,
        a: &Actor,
        s: Uuid,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn notifications(
        &self,
        a: &Actor,
        s: Uuid,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn read(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn cases(
        &self,
        a: &Actor,
        s: Uuid,
    ) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn case(
        &self,
        a: &Actor,
        s: Uuid,
        v: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn resolve(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Value,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}
