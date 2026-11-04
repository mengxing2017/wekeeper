use crate::{
    business::{Actor, Error, Page, Result, require, text},
    distribution::{
        domain::broadcast::*,
        south::{
            adapter::postgres::broadcast::Repository,
            port::broadcast::{Client, Command, Store},
        },
    },
    integrations::south::adapter::broadcast_http,
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<
    R = Repository,
    C = broadcast_http::Client,
    S = crate::distribution::south::adapter::in_process::Sources,
> {
    pub sources: S,
    pub store: R,
    pub client: C,
    pub management_available: bool,
    pub origin: String,
}
impl<R: Store, C: Client, S: crate::distribution::south::port::content_source::ContentSource>
    Service<R, C, S>
{
    pub async fn list(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Task>> {
        self.store.authorize(a.id, s, false).await?;
        self.store.list(s, page).await
    }
    pub async fn task(&self, a: &Actor, s: Uuid, id: Uuid) -> Result<Task> {
        self.store.authorize(a.id, s, false).await?;
        self.store.task(s, id).await
    }
    pub async fn rows(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        kind: &str,
        page: Page,
    ) -> Result<Vec<Value>> {
        self.store.authorize(a.id, s, false).await?;
        self.store.rows(s, id, kind, page).await
    }
    pub async fn save(
        &self,
        a: &Actor,
        s: Uuid,
        id: Option<Uuid>,
        input: DraftInput,
    ) -> Result<Task> {
        self.store
            .authorize(a.id, s, input.source_task_id.is_some())
            .await?;
        input.validate()?;
        if id.is_none()
            && let Some(task) = self.store.draft_request(a, s, &input).await?
        {
            return Ok(task);
        }
        let references = if input.content_source == ContentSource::Posts {
            self.store.source_references(s, &input.post_ids).await?
        } else {
            input.materials.clone()
        };
        let mut materials = self.sources.read(a, s, &references).await?;
        for material in &mut materials {
            if material.detail_url.starts_with('/') {
                material.detail_url = format!("{}{}", self.origin, material.detail_url);
            }
        }
        let scope = if self.management_available {
            self.client.generation().await.ok()
        } else {
            None
        };
        let (corp, generation) = scope
            .map(|(c, g)| (Some(c), Some(g)))
            .unwrap_or((None, None));
        self.store
            .execute(
                a,
                s,
                Command::Save {
                    id,
                    input,
                    materials,
                    corp,
                    generation,
                    preview_key: None,
                },
            )
            .await
    }
    pub async fn save_preview(
        &self,
        a: &Actor,
        s: Uuid,
        id: Option<Uuid>,
        mut input: DraftInput,
        key: &str,
    ) -> Result<Value> {
        self.store.authorize(a.id, s, false).await?;
        let key = text(key, 1, 128)?;
        input.validate()?;
        let input_hash =
            crate::business::hash(&serde_json::json!({"id":id,"input":input}).to_string());
        if let Some(prior) = self.store.preview_response(a, s, &key, &input_hash).await? {
            return Ok(prior);
        }
        // Drafts are retained even while delivery is unavailable; no external write occurs here.
        let references = if input.content_source == ContentSource::Posts {
            self.store.source_references(s, &input.post_ids).await?
        } else {
            input.materials.clone()
        };
        let mut materials = self.sources.read(a, s, &references).await?;
        for m in &mut materials {
            if m.detail_url.starts_with('/') {
                m.detail_url = format!("{}{}", self.origin, m.detail_url);
            }
        }
        let ready = self.management_available;
        let scope = if ready {
            self.client.generation().await.ok()
        } else {
            None
        };
        let (corp, generation) = scope
            .map(|(c, g)| (Some(c), Some(g)))
            .unwrap_or((None, None));
        if id.is_none() {
            input.request_key = Some(key.clone());
        }
        let command = Command::Save {
            id,
            input,
            materials,
            corp,
            generation,
            preview_key: Some(format!("{key}:{input_hash}")),
        };
        if let Err(error) = self.store.execute(a, s, command.clone()).await {
            // A failed resolution/preview transaction is rolled back. Retain the valid business
            // content and school-owned target references without inventing a sender or batch.
            if generation.is_none() {
                return Err(error);
            }
            if !matches!(
                error,
                Error::Invalid(_) | Error::Conflict | Error::Unavailable | Error::NotFound
            ) {
                return Err(error);
            }
            if let Command::Save {
                id,
                input,
                materials,
                preview_key,
                ..
            } = command
            {
                self.store
                    .execute(
                        a,
                        s,
                        Command::Save {
                            id,
                            input,
                            materials,
                            corp: None,
                            generation: None,
                            preview_key,
                        },
                    )
                    .await?;
            }
        }
        self.store
            .preview_response(a, s, &key, &input_hash)
            .await?
            .ok_or(Error::Conflict)
    }
    pub async fn evidence_candidates(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        batch: Uuid,
    ) -> Result<Value> {
        self.store.authorize(a.id, s, true).await?;
        let task = self.store.task(s, id).await?;
        let w = self
            .store
            .work(Some(id))
            .await?
            .into_iter()
            .find(|w| w.id == batch)
            .ok_or(Error::NotFound)?;
        let start = chrono::DateTime::parse_from_rfc3339(&task.created_at)
            .map_err(|_| Error::Conflict)?
            .timestamp()
            - 300;
        let end = chrono::Utc::now().timestamp();
        if end - start > 28 * 86400 {
            return Ok(
                serde_json::json!({"id":batch,"items":[],"reason":"已超出可查询时间，请移交负责人；未查到不能证明未发送"}),
            );
        }
        if !self.store.evidence_query(a, s, id, batch).await? {
            return Ok(
                serde_json::json!({"id":batch,"items":[],"reason":"刚刚查询过，请稍后再核对；当前状态仍保留不确定"}),
            );
        }
        let candidates = match self.client.candidates(start, end).await {
            Ok(v) => v,
            Err(_) => {
                return Ok(
                    serde_json::json!({"id":batch,"items":[],"reason":"暂时无法取得官方证据，请移交负责人或稍后重试"}),
                );
            }
        };
        let mut items = Vec::new();
        for v in candidates
            .into_iter()
            .filter(|v| v["text"]["content"].as_str() == Some(&task.content))
            .take(100)
        {
            let Some(msgid) = v["msgid"].as_str() else {
                continue;
            };
            let mut read = w.clone();
            read.msgid = Some(msgid.into());
            read.operation = "read".into();
            // The adapter rejects any member or target outside the frozen batch.
            let Ok(results) = self.client.results(&read).await else {
                continue;
            };
            if results.member_status.is_none()
                || results.groups.len() != w.chats.len()
                || !w
                    .chats
                    .iter()
                    .all(|chat| results.groups.iter().any(|g| &g.chat_id == chat))
            {
                continue;
            }
            let summary = serde_json::json!({"content":task.content,"sender":w.sender,"target_count":w.chats.len(),"created_at":v["create_time"],"member_status":results.member_status});
            if let Ok(candidate) = self
                .store
                .evidence_save(a, s, id, batch, msgid, summary)
                .await
            {
                items.push(candidate);
            }
        }
        Ok(
            serde_json::json!({"id":batch,"items":items,"reason":"仅列出目标及成员证据完整且属于本校的候选。候选不代表已发送；没有候选可移交负责人。"}),
        )
    }
    pub async fn preview(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
    ) -> Result<crate::distribution::domain::content::Preview> {
        self.store.authorize(a.id, s, false).await?;
        let mut task = self.store.task(s, id).await?;
        if task.total == 0 {
            let mut input = task.draft_input.clone();
            input.revision = Some(task.revision);
            input.request_key = None;
            task = self.save(a, s, Some(id), input).await?;
        }
        self.sources.check(s, &task.materials).await?;
        self.store.preview(a, s, id).await
    }
    pub async fn action(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        operation: &str,
        key: &str,
        mut input: Action,
    ) -> Result<Task> {
        self.store
            .authorize(a.id, s, matches!(operation, "stop" | "reconcile"))
            .await?;
        let key = text(key, 1, 128)?;
        require(!key.starts_with("child:"), "幂等键前缀保留")?;
        let task = self.store.task(s, id).await?;
        if self
            .store
            .action_replay(a, s, id, operation, &key, &input)
            .await?
        {
            return Ok(task);
        }
        if operation == "confirm"
            && task.content_source == crate::distribution::domain::broadcast::ContentSource::Text
        {
            self.store.authorize(a.id, s, true).await?;
        }
        if matches!(operation, "confirm" | "remind") {
            require(self.management_available, "群发需要有效企业微信服务端凭据")?;
        }
        if operation == "stop" {
            require(
                self.management_available,
                "群发管理需要有效企业微信服务端凭据",
            )?;
        }
        if operation == "confirm" {
            require(
                self.store.channel_ready().await?,
                "官方接口当前不可用或已暂停，系统将自动检查",
            )?;
        }
        if operation == "reconcile" && input.decision.as_deref() == Some("linked") {
            require(input.msgid.is_none(), "请从本校证据候选中选择核对记录")?;
            let evidence = input
                .evidence_id
                .ok_or_else(|| Error::Invalid("请选择本校证据候选".into()))?;
            input.msgid = Some(
                self.store
                    .evidence(a, s, id, input.batch_id.ok_or(Error::Conflict)?, evidence)
                    .await?,
            );
        }
        if operation == "confirm" && task.state == "draft" {
            self.sources.check(s, &task.materials).await?;
            require(input.revision == Some(task.revision), "草稿版本冲突")?;
            for w in self.store.work(Some(id)).await? {
                self.client.verify(&w).await?;
            }
        }
        if operation == "reconcile" && input.decision.as_deref() == Some("linked") {
            // Verify supplied msgid against the frozen sender/targets before associating it.
            let batch = input.batch_id.ok_or(Error::Conflict)?;
            let mut w = self
                .store
                .work(Some(id))
                .await?
                .into_iter()
                .find(|w| w.id == batch)
                .ok_or(Error::NotFound)?;
            w.msgid = input.msgid.clone();
            w.operation = "read".into();
            let start = chrono::DateTime::parse_from_rfc3339(&task.created_at)
                .map_err(|_| Error::Conflict)?
                .timestamp()
                - 300;
            let end = (start + 28 * 86400).min(chrono::Utc::now().timestamp());
            let candidates = self.client.candidates(start, end).await?;
            require(
                candidates.iter().any(|v| {
                    v["msgid"].as_str() == input.msgid.as_deref()
                        && v["text"]["content"].as_str() == Some(task.content.as_str())
                }),
                "官方任务内容与冻结正文不匹配",
            )?;
            let results = self.client.results(&w).await?;
            require(
                results.member_status.is_some()
                    && results.groups.len() == w.chats.len()
                    && w.chats
                        .iter()
                        .all(|chat| results.groups.iter().any(|g| &g.chat_id == chat)),
                "未取得本校完整成员和目标证据，请移交负责人",
            )?;
        }
        self.store
            .execute(
                a,
                s,
                Command::Action {
                    id,
                    operation: operation.into(),
                    key,
                    input,
                },
            )
            .await
    }
    pub async fn candidates(&self, a: &Actor, start: i64, end: i64) -> Result<Vec<Value>> {
        a.platform()?;
        self.client.candidates(start, end).await
    }
    pub async fn run_once(&self) -> Result<()> {
        // Only explicitly confirmed work is claimed; credentials are required for upstream calls.
        if !self.management_available {
            return Ok(());
        }
        for w in self.store.work(None).await? {
            if w.operation == "read" {
                if self.store.claim(&w).await? {
                    self.store
                        .observed(&w, self.client.results(&w).await)
                        .await?;
                }
                continue;
            }
            let source_ok = if matches!(w.operation.as_str(), "create" | "remind") {
                let task = self.store.task(w.school_id, w.task_id).await?;
                self.sources
                    .check(w.school_id, &task.materials)
                    .await
                    .is_ok()
            } else {
                true
            };
            if !source_ok || self.client.verify(&w).await.is_err() {
                let preflight = Work {
                    operation: if w.operation == "create" {
                        "preflight".into()
                    } else {
                        w.operation.clone()
                    },
                    ..w.clone()
                };
                self.store
                    .finish(&preflight, Outcome::Rejected("preflight_failed".into()))
                    .await?;
                continue;
            }
            match self.store.claim(&w).await {
                Ok(true) => {
                    self.store.finish(&w, self.client.invoke(&w).await).await?;
                }
                Ok(false) => {}
                Err(Error::Forbidden | Error::Conflict | Error::Invalid(_) | Error::Policy(_)) => {
                    self.store
                        .finish(
                            &Work {
                                operation: if w.operation == "create" {
                                    "preflight".into()
                                } else {
                                    w.operation.clone()
                                },
                                ..w.clone()
                            },
                            Outcome::Rejected("authorization_or_snapshot_changed".into()),
                        )
                        .await?;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
fn unavailable_reason(configured: bool) -> Option<&'static str> {
    (!configured).then_some("官方渠道服务端凭据未配置，可保存草稿。")
}

impl crate::bootstrap::Application {
    pub fn broadcast_unavailable_reason(&self) -> Option<&'static str> {
        unavailable_reason(self.integrations.server_configured())
    }
    pub fn broadcast(&self) -> Service {
        // Server-to-server operations do not depend on browser origin or OAuth login.
        let management_available = self.integrations.server_configured();
        Service {
            sources: crate::distribution::south::adapter::in_process::Sources(self.pool.clone()),
            store: Repository(self.pool.clone()),
            client: broadcast_http::Client(self.integrations.clone()),
            management_available,
            origin: self.options.public_origin.clone(),
        }
    }
}

#[cfg(test)]
mod availability_tests {
    use super::unavailable_reason;
    #[test]
    fn server_credentials_make_broadcast_available_without_a_deployment_switch() {
        assert!(unavailable_reason(false).unwrap().contains("凭据未配置"));
        assert!(unavailable_reason(true).is_none());
    }
}
