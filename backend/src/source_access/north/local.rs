use crate::source_access::south::port::MachineStore;
use crate::source_access::{domain::*, south::adapter::postgres::Repository};
use serde_json::Value;
use sqlx::PgPool;
#[derive(Clone)]
pub struct Service(pub PgPool);
pub struct MachineService<S>(pub S);
impl<S: MachineStore> MachineService<S> {
    pub async fn machine(&self, signed: Signed, body: Vec<u8>) -> Result<(bool, Value)> {
        let repo = &self.0;
        let key = signed.key_id.clone();
        let request_id = signed.request_id.clone();
        let result = repo.accept(signed, body).await;
        if let Err(e) = &result {
            let _ = repo.record_failure(&key, &request_id, e.0).await;
        }
        result
    }
}
impl Service {
    #[allow(clippy::too_many_arguments)]
    pub async fn manage(
        &self,
        actor: &crate::business::Actor,
        kind: &str,
        id: Option<&str>,
        method: &str,
        input: Option<AdminInput>,
        page: i64,
        search: &str,
        status: &str,
        source: Option<uuid::Uuid>,
    ) -> Result<Value> {
        if method == "GET" || !actor.platform_admin {
            Repository(self.0.clone())
                .management_read_audit(actor, kind)
                .await?;
        }
        ensure(actor.platform_admin, "forbidden")?;
        Repository(self.0.clone())
            .manage(
                actor,
                kind,
                id,
                method,
                input,
                page,
                search,
                status,
                source,
                (None, None),
            )
            .await
    }
}

impl Service {
    pub async fn list(
        &self,
        actor: &crate::business::Actor,
        kind: &str,
        q: ManagementFilter,
    ) -> Result<Value> {
        let repo = Repository(self.0.clone());
        repo.management_read_audit(actor, kind).await?;
        ensure(actor.platform_admin, "forbidden")?;
        let source = if let Some(key) = q.key_id.as_deref() {
            Some(repo.caller_for_key(key).await?)
        } else {
            None
        };
        repo.manage(
            actor,
            kind,
            None,
            "GET",
            None,
            q.page.unwrap_or(1),
            q.search.as_deref().unwrap_or(""),
            q.status.as_deref().unwrap_or(""),
            source,
            (q.valid_from, q.valid_until),
        )
        .await
    }
}

pub mod routing;

pub mod projection;

pub mod keyword_filter;

pub mod orchestration;
