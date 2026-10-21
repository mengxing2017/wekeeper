use super::super::{
    domain::{CommentInput, PostInput},
    south::adapter::postgres::Repository,
};
use crate::business::{Actor, Decision, Page, Result, require, text};
use crate::community::south::port::Store;
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn post(&self, a: &Actor, s: Uuid, id: Option<Uuid>, v: PostInput) -> Result<Value> {
        a.authorize(s, "member")?;
        v.validate(s)?;
        self.0.validate_media(s, &v.images).await?;
        self.0
            .check_words(s, &format!("{} {}", v.title, v.content))
            .await?;
        if let Some(id) = id {
            self.0.update(a, s, id, v).await
        } else {
            self.0.create(a, s, v).await
        }
    }
    pub async fn visibility(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        a.authorize(s, "member")?;
        require(
            ["visible", "hidden", "deleted"].contains(&v["visibility"].as_str().unwrap_or("")),
            "可见状态无效",
        )?;
        self.0.visibility(a, s, id, v).await
    }
    pub async fn moderate(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        v: Decision,
        comment: bool,
    ) -> Result<Value> {
        a.authorize(s, "operator")?;
        require(
            ["approved", "rejected", "pending"].contains(&v.decision.as_str()),
            "审核状态无效",
        )?;
        require(v.reason.len() <= 2000, "原因过长")?;
        self.0.moderate(a, s, id, v, comment).await
    }
    pub async fn comment(&self, a: &Actor, s: Uuid, id: Uuid, v: CommentInput) -> Result<Value> {
        a.authorize(s, "member")?;
        text(&v.content, 1, 2000)?;
        self.0.check_words(s, &v.content).await?;
        self.0.comment(a, s, id, v).await
    }
    pub async fn case(&self, a: &Actor, s: Uuid, v: Value) -> Result<Value> {
        a.authorize(s, "member")?;
        let kind = v["kind"].as_str().unwrap_or("");
        require(["report", "feedback"].contains(&kind), "类型无效")?;
        text(v["content"].as_str().unwrap_or(""), 1, 500)?;
        if kind == "report" {
            crate::business::uuid(&v, "post_id")?;
        } else {
            text(v["contact"].as_str().unwrap_or(""), 0, 200)?;
        }
        if let Some(images) = v.get("images") {
            let paths: Vec<String> = serde_json::from_value(images.clone())
                .map_err(|_| crate::business::Error::Invalid("图片格式错误".into()))?;
            require(paths.len() <= 9, "最多9张图片")?;
            self.0.validate_media(s, &paths).await?;
        }
        self.0.case(a, s, v).await
    }
}

impl<R: Store> Service<R> {
    pub async fn posts(&self, a: &Actor, s: Uuid, p: crate::business::Page) -> Result<Vec<Value>> {
        a.authorize(s, "member")?;
        self.0.posts(a, s, p).await
    }
    pub async fn detail(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<Value> {
        a.authorize(s, "member")?;
        self.0.detail(a, s, id).await
    }
    pub async fn comments(
        &self,
        a: &Actor,
        s: Uuid,
        id: Option<Uuid>,
        p: crate::business::Page,
    ) -> Result<Vec<Value>> {
        a.authorize(s, if id.is_some() { "member" } else { "operator" })?;
        self.0.comments(a, s, id, p).await
    }
    pub async fn delete_comment(&self, a: &Actor, s: Uuid, id: Uuid, rev: i64) -> Result<()> {
        a.authorize(s, "member")?;
        self.0.delete_comment(a, s, id, rev).await
    }
    pub async fn react(&self, a: &Actor, s: Uuid, id: Uuid, kind: &str, add: bool) -> Result<()> {
        a.authorize(s, "member")?;
        require(["like", "collect"].contains(&kind), "互动类型无效")?;
        self.0.react(a, s, id, kind, add).await
    }
    pub async fn reactions(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "member")?;
        self.0.reactions(a, s).await
    }
    pub async fn follow(&self, a: &Actor, s: Uuid, id: Uuid, add: bool) -> Result<()> {
        a.authorize(s, "member")?;
        require(a.id != id, "不能关注自己")?;
        self.0.follow(a, s, id, add).await
    }
    pub async fn follows(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "member")?;
        self.0.follows(a, s).await
    }
    pub async fn notifications(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Value>> {
        a.authorize(s, "member")?;
        self.0.notifications(a, s, page).await
    }
    pub async fn read(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<()> {
        a.authorize(s, "member")?;
        self.0.read(a, s, id).await
    }
    pub async fn cases(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Value>> {
        a.authorize(s, "member")?;
        self.0.cases(a, s, page).await
    }
    pub async fn resolve(&self, a: &Actor, s: Uuid, id: Uuid, v: Value) -> Result<Value> {
        a.authorize(s, "operator")?;
        require(
            ["resolved", "dismissed"].contains(&v["state"].as_str().unwrap_or("")),
            "处理状态无效",
        )?;
        text(v["resolution"].as_str().unwrap_or(""), 1, 2000)?;
        self.0.resolve(a, s, id, v).await
    }
}
