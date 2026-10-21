use crate::{
    business::{Actor, Error, Result, require},
    integrations::{
        domain::directory::*,
        south::{
            adapter::{directory_http::Client, directory_postgres::Repository},
            port::{DirectoryClient, DirectoryStore},
        },
    },
};
use serde_json::Value;
use tracing::Instrument;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R = Repository, C = Client> {
    pub store: R,
    pub client: C,
    pub corp: String,
    pub credential_scope: String,
    pub server_configured: bool,
}
impl<R: DirectoryStore, C: DirectoryClient> Service<R, C> {
    pub async fn ensure_config(&self) -> Result<i64> {
        require(self.server_configured, "企业微信凭据尚未配置")?;
        self.store
            .configure(&self.corp, &self.credential_scope)
            .await
    }
    pub async fn status(&self, a: &Actor) -> Result<Value> {
        a.platform()?;
        let error = self.ensure_config().await.err().map(|e| match e {
            Error::Invalid(m) => m,
            _ => "目录配置检查失败".into(),
        });
        let mut v = self
            .store
            .status(&self.corp, &self.credential_scope)
            .await?;
        v["server_configured"] = self.server_configured.into();
        v["configuration_error"] = error.into();
        Ok(v)
    }
    pub async fn list(
        &self,
        a: &Actor,
        q: &DirectoryQuery,
        departments: bool,
    ) -> Result<Vec<Value>> {
        a.platform()?;
        q.validate()?;
        self.store.list(q, departments).await
    }
    pub async fn enqueue(&self, a: &Actor) -> Result<Value> {
        a.platform()?;
        let generation = self.ensure_config().await?;
        let job = self.store.enqueue(Some(a.id), 0, generation).await?;
        tracing::info!(
            job_id = job["id"].as_str().unwrap_or("unknown"),
            generation,
            state = job["state"].as_str().unwrap_or("unknown"),
            "wecom directory sync requested; queued or existing active job returned"
        );
        Ok(job)
    }
    pub async fn job(&self, a: &Actor, id: Uuid) -> Result<Value> {
        a.platform()?;
        self.store.job(id).await
    }
    pub async fn verify(&self, userid: &str) -> Result<i64> {
        let generation = self.ensure_config().await?;
        let key = userid_key(userid)?;
        let at = chrono::Utc::now();
        match self.client.member(userid).await {
            Ok(m) => {
                m.validate()?;
                require(userid_key(&m.userid)? == key, "企业成员身份不匹配")?;
                self.store.observe(userid, Some(&m), at, generation).await?;
                require(m.status == Some(1), "企业成员未激活、停用或状态待核验")?;
                Ok(generation)
            }
            Err(e) => {
                if e.unavailable_member() {
                    self.store.observe(userid, None, at, generation).await?;
                }
                Err(Error::Invalid(format!("企业成员核验失败：{}", e.code())))
            }
        }
    }
    pub async fn verify_account(&self, id: Uuid) -> Result<()> {
        let userid = self.store.account_userid(id).await?;
        self.verify(&userid).await.map(|_| ())
    }
    pub async fn bind(&self, a: &Actor, id: Uuid, input: BindingInput) -> Result<()> {
        a.platform()?;
        require(input.revision >= 0, "绑定版本无效")?;
        let userid = self.store.member_id(input.member_id).await?;
        let generation = self.verify(&userid).await?;
        self.store
            .bind(
                a.id,
                id,
                Some(input.member_id),
                input.revision,
                Some(generation),
            )
            .await
    }
    pub async fn unbind(&self, a: &Actor, id: Uuid, revision: i64) -> Result<()> {
        a.platform()?;
        require(revision >= 0, "绑定版本无效")?;
        self.store.bind(a.id, id, None, revision, None).await
    }
    pub async fn rotate(&self, a: &Actor, input: CredentialRotation) -> Result<()> {
        a.platform()?;
        require(self.server_configured, "企业微信服务端凭据尚未配置")?;
        require(input.expected_generation > 0, "凭据代次无效")?;
        self.store
            .check_rotation(
                &self.corp,
                &self.credential_scope,
                input.expected_generation,
            )
            .await?;
        // Validate the new credentials and visible directory before the atomic transition.
        self.collect()
            .await
            .map_err(|e| Error::Invalid(format!("新凭据目录验证失败：{}", e.code())))?;
        self.store
            .rotate(
                a.id,
                &self.corp,
                &self.credential_scope,
                input.expected_generation,
            )
            .await
    }
    async fn collect(&self) -> std::result::Result<(Vec<Department>, Vec<Member>), ReadError> {
        let departments = self.client.departments().await?;
        // An empty department scope is not proof that every employee left.
        if departments.is_empty() {
            return Err(ReadError::Rejected(60011));
        }
        let mut members = std::collections::BTreeMap::<String, Member>::new();
        for d in &departments {
            for mut m in self.client.members(d.id).await? {
                m.validate().map_err(|_| ReadError::Invalid)?;
                let key = userid_key(&m.userid).map_err(|_| ReadError::Invalid)?;
                if let Some(previous) = members.get(&key) {
                    if previous.name != m.name || previous.status != m.status {
                        return Err(ReadError::Invalid);
                    }
                    m.department.extend(&previous.department);
                }
                m.department.sort();
                m.department.dedup();
                members.insert(key, m);
            }
        }
        // Bound members can be individually visible even when their department is not.
        for userid in self
            .store
            .bound_userids()
            .await
            .map_err(|_| ReadError::Transient)?
        {
            let key = userid_key(&userid).map_err(|_| ReadError::Invalid)?;
            if members.contains_key(&key) {
                continue;
            }
            match self.client.member(&userid).await {
                Ok(m) => {
                    m.validate().map_err(|_| ReadError::Invalid)?;
                    if userid_key(&m.userid).map_err(|_| ReadError::Invalid)? != key {
                        return Err(ReadError::Invalid);
                    }
                    members.insert(key, m);
                }
                Err(e) if e.unavailable_member() => {}
                Err(e) => return Err(e),
            }
        }
        Ok((departments, members.into_values().collect()))
    }
    pub async fn run_once(&self, interval: i64) -> Result<()> {
        let generation = self.ensure_config().await?;
        if interval > 0 {
            self.store.enqueue(None, interval, generation).await?;
        }
        let Some(job) = self.store.claim(generation).await? else {
            return Ok(());
        };
        self.run_job(&job)
            .instrument(tracing::info_span!("wecom_directory_sync", job_id = %job.id, generation = job.generation))
            .await
    }
    async fn run_job(&self, job: &Job) -> Result<()> {
        let started = std::time::Instant::now();
        tracing::info!(state = "running", "wecom directory sync started");
        let collect = self.collect();
        tokio::pin!(collect);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(20));
        // interval's first tick is immediate; do not report progress before any work.
        heartbeat.tick().await;
        let result = loop {
            tokio::select! {
                result=&mut collect=>break result,
                _=heartbeat.tick()=> {
                    if let Err(e) = self.store.renew(job).await {
                        tracing::warn!(stage = "lease_renewal", error_code = error_code(&e), elapsed_ms = started.elapsed().as_millis() as u64,
                            "wecom directory sync stopped; lease renewal failed, this worker will not publish");
                        return Err(e);
                    }
                    tracing::info!(state = "running", stage = "collect", elapsed_ms = started.elapsed().as_millis() as u64,
                        "wecom directory sync in progress; collecting visible directory");
                },
            }
        };
        match result {
            Ok((departments, members)) => {
                if let Err(e) = self.store.finish(job, &departments, &members).await {
                    tracing::warn!(
                        stage = "publish",
                        error_code = error_code(&e),
                        elapsed_ms = started.elapsed().as_millis() as u64,
                        "wecom directory sync publication failed; success not confirmed"
                    );
                    self.record_failure(job, "publication_failed").await?;
                    return Err(e);
                }
                // Only announce success after the publication transaction has committed.
                tracing::info!(
                    state = "succeeded",
                    department_count = departments.len(),
                    member_count = members.len(),
                    elapsed_ms = started.elapsed().as_millis() as u64,
                    "wecom directory sync succeeded; snapshot committed"
                );
            }
            Err(e) => {
                let code = e.code();
                tracing::warn!(stage = "collect", error_code = %code, elapsed_ms = started.elapsed().as_millis() as u64,
                    "wecom directory sync failed; previous snapshot retained");
                self.record_failure(job, &code).await?;
            }
        }
        Ok(())
    }
    async fn record_failure(&self, job: &Job, code: &str) -> Result<()> {
        self.store.fail(job, code).await.inspect_err(|e| {
            tracing::warn!(
                stage = "record_failure",
                error_code = error_code(e),
                "wecom directory sync failure status could not be saved; inspect task status"
            );
        })
    }
}

// Do not log Error's Debug/Display: database or validation details may contain sensitive data.
pub(crate) fn error_code(error: &Error) -> &'static str {
    match error {
        Error::Unauthorized => "unauthorized",
        Error::Forbidden => "forbidden",
        Error::NotFound => "not_found",
        Error::Conflict => "conflict_or_stale_generation",
        Error::Invalid(_) => "invalid_configuration_or_input",
        Error::Unavailable | Error::Upstream(_) => "upstream_unavailable",
        Error::Database(_) => "database_error",
    }
}
