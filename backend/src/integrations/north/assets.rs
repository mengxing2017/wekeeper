use crate::{
    business::{Actor, Error, Result, require},
    integrations::{
        domain::{
            assets::*,
            directory::{ReadError, userid_key},
        },
        south::{
            adapter::{assets_postgres::Repository, directory_http::Client},
            port::assets::{AssetClient, AssetStore},
        },
    },
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<C: AssetClient = Client, R: AssetStore = Repository> {
    pub store: R,
    pub client: C,
    pub integration: super::local::Service,
}
fn upstream(e: ReadError) -> Error {
    Error::Invalid(e.code())
}
fn key(s: &str) -> Result<String> {
    userid_key(s)
}
fn cursor(next: String, seen: &mut BTreeSet<String>) -> Result<Option<String>> {
    if next.is_empty() {
        Ok(None)
    } else if next.len() > 16384 || !seen.insert(next.clone()) {
        Err(Error::Invalid("cursor_loop".into()))
    } else {
        Ok(Some(next))
    }
}
impl<C: AssetClient, R: AssetStore> Service<C, R> {
    pub async fn status(&self, a: &Actor) -> Result<Value> {
        a.platform()?;
        let configured = self.integration.server_configured();
        let error = self
            .integration
            .directory()
            .ensure_config()
            .await
            .err()
            .map(|e| match e {
                Error::Invalid(message) => message,
                _ => "配置状态读取失败，请重试".into(),
            });
        let mut v = self.store.status().await?;
        v["server_configured"] = json!(configured);
        v["configuration_error"] = error.map(|e| json!(e)).unwrap_or(Value::Null);
        Ok(v)
    }
    pub async fn confirm_scope(&self, a: &Actor, mut input: OwnerScopeDecision) -> Result<()> {
        a.platform()?;
        let g = self.integration.directory().ensure_config().await?;
        if g != input.expected_generation {
            return Err(Error::Conflict);
        }
        input.reason = crate::business::text(&input.reason, 5, 500)?;
        self.store.confirm_scope(a, &input).await
    }
    pub async fn enqueue(&self, a: &Actor, input: SyncInput) -> Result<Vec<Value>> {
        a.platform()?;
        input.validate()?;
        let g = self.integration.directory().ensure_config().await?;
        let runs = self.store.enqueue(Some(a.id), g, &input.dataset, 0).await?;
        for r in &runs {
            tracing::info!(
                dataset = r["dataset"].as_str(),
                job_id = r["id"].as_str(),
                generation = g,
                state = r["state"].as_str(),
                "wecom asset sync requested"
            );
        }
        Ok(runs)
    }
    pub async fn list(
        &self,
        a: &Actor,
        kind: &str,
        q: &AssetQuery,
        parent: Option<Uuid>,
        id: Option<Uuid>,
    ) -> Result<Vec<Value>> {
        a.platform()?;
        q.validate()?;
        require(
            q.member_id.is_none() || kind == "member-statistics",
            "当前资源不支持 member_id",
        )?;
        require(
            (q.owner_member_id.is_none()
                && q.participant_member_id.is_none()
                && q.associated.is_none())
                || kind == "groups",
            "当前资源不支持群筛选",
        )?;
        require(
            q.dataset.is_none() || kind == "sync-runs",
            "当前资源不支持 dataset",
        )?;
        require(
            q.verification.is_none() || !["sync-runs", "member-statistics"].contains(&kind),
            "当前资源不支持核验筛选",
        )?;
        require(
            q.search.is_none() || kind != "sync-runs",
            "任务列表不支持搜索",
        )?;
        self.store.list(kind, q, parent, id).await
    }
    pub async fn candidates(&self, a: &Actor, school: Uuid, q: &AssetQuery) -> Result<Vec<Value>> {
        a.authorize(school, "admin")?;
        q.validate()?;
        require(
            q.member_id.is_none()
                && q.owner_member_id.is_none()
                && q.participant_member_id.is_none()
                && q.verification.is_none()
                && q.associated.is_none()
                && q.dataset.is_none(),
            "候选只支持搜索和分页",
        )?;
        let g = self.integration.directory().ensure_config().await?;
        if !a.platform_admin {
            self.integration.directory().verify_account(a.id).await?;
        }
        self.store.candidates(a, school, q, g).await
    }
    pub async fn bind(&self, a: &Actor, school: Uuid, input: GroupBinding) -> Result<Value> {
        a.authorize(school, "admin")?;
        let g = self.integration.directory().ensure_config().await?;
        if g != input.expected_generation {
            return Err(Error::Conflict);
        }
        if !a.platform_admin {
            self.integration.directory().verify_account(a.id).await?;
        }
        let row = self
            .store
            .candidate(a, school, input.directory_group_id, g)
            .await?;
        if row["checked_at"]
            .as_str()
            .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
            .map(|v| v.with_timezone(&chrono::Utc))
            != Some(input.expected_checked_at)
        {
            return Err(Error::Conflict);
        }
        let id = row["chat_id"].as_str().ok_or(Error::Conflict)?;
        let group = self.client.group(id).await.map_err(upstream)?;
        group.validate(id).map_err(upstream)?;
        self.store.bind_group(a, school, &input, &group).await
    }
    pub async fn run_once(&self, interval: i64) -> Result<()> {
        let g = self.integration.directory().ensure_config().await?;
        if interval > 0 {
            self.store.enqueue(None, g, "groups", interval).await?;
        }
        let Some(j) = self.store.claim(g).await? else {
            return Ok(());
        };
        self.run_job(&j).await
    }
    pub async fn run_job(&self, j: &AssetJob) -> Result<()> {
        let started = std::time::Instant::now();
        tracing::info!(dataset=j.dataset,job_id=%j.fence.id,generation=j.fence.generation,"wecom asset sync started");
        let collection = async {
            require(j.dataset == "groups", "不支持的数据集")?;
            self.groups(j).await?;
            self.store.complete(j).await?;
            self.store.finish(j).await
        };
        tokio::pin!(collection);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(20));
        heartbeat.tick().await;
        let result = loop {
            tokio::select! {r=&mut collection=>break r,_=heartbeat.tick()=>{let p=self.store.renew(j).await?;tracing::info!(dataset=j.dataset,job_id=%j.fence.id,generation=j.fence.generation,stage=p["stage"].as_str(),processed=p["processed"].as_i64(),total=p["total"].as_i64(),pages=p["pages"].as_i64(),elapsed_ms=started.elapsed().as_millis() as u64,"wecom asset sync progress");}}
        };
        match result {
            Ok(()) => {
                let counts = self
                    .store
                    .list("sync-runs", &AssetQuery::default(), None, Some(j.fence.id))
                    .await
                    .ok()
                    .and_then(|rows| rows.into_iter().next())
                    .unwrap_or(Value::Null);
                tracing::info!(dataset=j.dataset,job_id=%j.fence.id,generation=j.fence.generation,stage="published",processed=counts["processed"].as_i64(),total=counts["total"].as_i64(),pages=counts["pages"].as_i64(),elapsed_ms=started.elapsed().as_millis() as u64,"wecom asset sync succeeded; snapshot committed");
                Ok(())
            }
            Err(e) => {
                let code = match &e {
                    Error::Invalid(c)
                        if c.starts_with("wecom_")
                            || [
                                "cursor_loop",
                                "invalid_response",
                                "network_or_rate_limit",
                                "directory_snapshot_required",
                                "inconsistent_snapshot",
                                "owner_scope_incomplete",
                            ]
                            .contains(&c.as_str()) =>
                    {
                        c.as_str()
                    }
                    _ => super::directory::error_code(&e),
                };
                tracing::warn!(dataset=j.dataset,job_id=%j.fence.id,generation=j.fence.generation,error_code=code,elapsed_ms=started.elapsed().as_millis() as u64,"wecom asset sync failed; previous snapshot retained");
                self.store.fail(j, code).await?;
                Ok(())
            }
        }
    }
    async fn stage_group(
        &self,
        j: &AssetJob,
        g: Group,
        status: Option<i64>,
        point: bool,
    ) -> Result<()> {
        g.validate(&g.chat_id).map_err(upstream)?;
        let mut data = value(&g);
        data.as_object_mut().unwrap().remove("member_list");
        data["member_key"] = json!(key(&g.owner)?);
        data["member_count"] = json!(g.member_count());
        data["status"] = json!(status);
        data["point_verification"] = json!(point);
        self.store.stage(j, "groups", &g.chat_id, data).await?;
        for m in g.member_list {
            let k = if m.r#type == 1 {
                key(&m.userid)?
            } else {
                m.userid.clone()
            };
            let mut data = value(&m);
            data["parent_key"] = json!(g.chat_id);
            data["member_key"] = json!(k);
            data["member_type"] = json!(m.r#type);
            data["point_verification"] = json!(point);
            self.store
                .stage(
                    j,
                    "members",
                    &edge(&g.chat_id, &edge(&m.r#type.to_string(), &k)),
                    data,
                )
                .await?;
        }
        Ok(())
    }
    async fn groups(&self, j: &AssetJob) -> Result<()> {
        let owners: BTreeMap<String, String> = self
            .store
            .owners(j.fence.generation)
            .await?
            .into_iter()
            .map(|id| Ok((key(&id)?, id)))
            .collect::<Result<_>>()?;
        self.store
            .scope(j, &owners.keys().cloned().collect::<Vec<_>>())
            .await?;
        let owners: Vec<_> = owners.into_values().collect();
        for chunk in owners.chunks(100) {
            self.group_chunk(j, chunk).await?;
        }
        let mut after = String::new();
        loop {
            let batch = self.store.items(j, "group-ids", &after).await?;
            if batch.is_empty() {
                break;
            }
            for (id, data) in batch {
                let g = self.client.group(&id).await.map_err(upstream)?;
                g.validate(&id).map_err(upstream)?;
                self.stage_group(j, g, data["status"].as_i64(), false)
                    .await?;
                after = id;
            }
        }
        for id in self.store.configured_groups().await? {
            if !self.store.contains(j, "group-ids", &id).await? {
                let g = self.client.group(&id).await.map_err(upstream)?;
                g.validate(&id).map_err(upstream)?;
                self.stage_group(j, g, None, true).await?;
            }
        }
        Ok(())
    }
    async fn group_chunk(&self, j: &AssetJob, owners: &[String]) -> Result<()> {
        let mut cur = String::new();
        let mut seen = BTreeSet::new();
        loop {
            let p = match self.client.groups(owners, &cur).await {
                Ok(p) => p,
                Err(e) => {
                    // Diagnose each owner in a rejected batch, but never claim complete coverage after a skipped owner.
                    if owners.len() > 1
                        && matches!(e, ReadError::Rejected(60011 | 60111 | 60121 | 84061))
                    {
                        for owner in owners {
                            if let Err(error) =
                                self.client.groups(std::slice::from_ref(owner), "").await
                            {
                                if matches!(
                                    error,
                                    ReadError::Rejected(60011 | 60111 | 60121 | 84061)
                                ) {
                                    self.store.failed_owner(j, owner).await?;
                                }
                                return Err(upstream(error));
                            }
                        }
                    }
                    if owners.len() == 1
                        && matches!(e, ReadError::Rejected(60011 | 60111 | 60121 | 84061))
                    {
                        self.store.failed_owner(j, &owners[0]).await?;
                    }
                    return Err(upstream(e));
                }
            };
            for g in p.group_chat_list {
                require(opaque(&g.chat_id), "invalid_response")?;
                self.store
                    .stage(j, "group-ids", &g.chat_id, json!({"status":g.status}))
                    .await?;
            }
            self.store.page(j).await?;
            match cursor(p.next_cursor, &mut seen)? {
                Some(next) => cur = next,
                None => break,
            }
        }
        Ok(())
    }
}
