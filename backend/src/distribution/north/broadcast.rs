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
pub struct Service<R = Repository, C = broadcast_http::Client> {
    pub store: R,
    pub client: C,
    pub enabled: bool,
    pub management_available: bool,
    pub origin: String,
}
impl<R: Store, C: Client> Service<R, C> {
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
        let (corp, generation) = self.client.generation().await?;
        self.store
            .execute(
                a,
                s,
                Command::Save {
                    id,
                    input,
                    origin: self.origin.clone(),
                    corp,
                    generation,
                },
            )
            .await
    }
    pub async fn action(
        &self,
        a: &Actor,
        s: Uuid,
        id: Uuid,
        operation: &str,
        key: &str,
        input: Action,
    ) -> Result<Task> {
        self.store
            .authorize(a.id, s, matches!(operation, "stop" | "reconcile"))
            .await?;
        let key = text(key, 1, 128)?;
        require(!key.starts_with("child:"), "幂等键前缀保留")?;
        let task = self.store.task(s, id).await?;
        if operation == "confirm"
            && task.content_source == crate::distribution::domain::broadcast::ContentSource::Text
        {
            self.store.authorize(a.id, s, true).await?;
        }
        if matches!(operation, "confirm" | "remind") {
            require(
                self.enabled,
                "企业群发尚未启用；请核验权限及成员终端版本后启用",
            )?;
        }
        if operation == "stop" {
            require(
                self.management_available,
                "群发管理需要有效企业凭据和 HTTPS 部署",
            )?;
        }
        if operation == "confirm" && task.state == "draft" {
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
            require(
                self.client.results(&w).await?.member_status.is_some(),
                "未取得指定执行成员的任务证据",
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
        // Disabled also in embedded mode: merely exporting a Secret never enables writes.
        if !self.management_available {
            return Ok(());
        }
        for w in self.store.work(None).await? {
            if !self.enabled && matches!(w.operation.as_str(), "create" | "remind") {
                continue;
            }
            if w.operation == "read" {
                if self.store.claim(&w).await? {
                    self.store
                        .observed(&w, self.client.results(&w).await)
                        .await?;
                }
                continue;
            }
            if self.client.verify(&w).await.is_err() {
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
                Err(Error::Forbidden | Error::Conflict | Error::Invalid(_)) => {
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
impl crate::bootstrap::Application {
    pub fn broadcast(&self) -> Service {
        let management_available = self
            .integrations
            .capabilities(
                &self.options.public_origin,
                self.options.insecure_local_auth,
            )
            .oauth_available;
        let enabled = management_available
            && std::env::var("WEKEEPER_WECOM_BROADCAST_ENABLED").is_ok_and(|v| v == "true");
        Service {
            store: Repository(self.pool.clone()),
            client: broadcast_http::Client(self.integrations.clone()),
            enabled,
            management_available,
            origin: self.options.public_origin.clone(),
        }
    }
}
