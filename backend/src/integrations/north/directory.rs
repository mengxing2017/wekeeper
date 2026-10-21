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
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R = Repository, C = Client> {
    pub store: R,
    pub client: C,
    pub corp: String,
    pub agent: String,
    pub configured: bool,
}
impl<R: DirectoryStore, C: DirectoryClient> Service<R, C> {
    pub async fn ensure_config(&self) -> Result<()> {
        require(self.configured, "企业微信凭据尚未配置")?;
        self.store.configure(&self.corp, &self.agent).await
    }
    pub async fn status(&self, a: &Actor) -> Result<Value> {
        a.platform()?;
        let error = self.ensure_config().await.err().map(|e| match e {
            Error::Invalid(m) => m,
            _ => "目录配置检查失败".into(),
        });
        let mut v = self.store.status().await?;
        v["configured"] = self.configured.into();
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
        self.ensure_config().await?;
        self.store.enqueue(Some(a.id), 0).await
    }
    pub async fn job(&self, a: &Actor, id: Uuid) -> Result<Value> {
        a.platform()?;
        self.store.job(id).await
    }
    pub async fn verify(&self, userid: &str) -> Result<()> {
        self.ensure_config().await?;
        let key = userid_key(userid)?;
        let at = chrono::Utc::now();
        match self.client.member(userid).await {
            Ok(m) => {
                m.validate()?;
                require(userid_key(&m.userid)? == key, "企业成员身份不匹配")?;
                self.store.observe(userid, Some(&m), at).await?;
                require(m.status == Some(1), "企业成员未激活、停用或状态待核验")
            }
            Err(e) => {
                if e.unavailable_member() {
                    self.store.observe(userid, None, at).await?;
                }
                Err(Error::Invalid(format!("企业成员核验失败：{}", e.code())))
            }
        }
    }
    pub async fn verify_account(&self, id: Uuid) -> Result<()> {
        let userid = self.store.account_userid(id).await?;
        self.verify(&userid).await
    }
    pub async fn bind(&self, a: &Actor, id: Uuid, input: BindingInput) -> Result<()> {
        a.platform()?;
        require(input.revision >= 0, "绑定版本无效")?;
        let userid = self.store.member_id(input.member_id).await?;
        self.verify(&userid).await?;
        self.store
            .bind(a.id, id, Some(input.member_id), input.revision)
            .await
    }
    pub async fn unbind(&self, a: &Actor, id: Uuid, revision: i64) -> Result<()> {
        a.platform()?;
        require(revision >= 0, "绑定版本无效")?;
        self.store.bind(a.id, id, None, revision).await
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
        self.ensure_config().await?;
        if interval > 0 {
            self.store.enqueue(None, interval).await?;
        }
        let Some(job) = self.store.claim().await? else {
            return Ok(());
        };
        let collect = self.collect();
        tokio::pin!(collect);
        let mut heartbeat = tokio::time::interval(std::time::Duration::from_secs(20));
        let result = loop {
            tokio::select! {
                result=&mut collect=>break result,
                _=heartbeat.tick()=>self.store.renew(&job).await?,
            }
        };
        match result {
            Ok((departments, members)) => {
                if let Err(e) = self.store.finish(&job, &departments, &members).await {
                    self.store.fail(&job, "publication_failed").await?;
                    return Err(e);
                }
            }
            Err(e) => self.store.fail(&job, &e.code()).await?,
        }
        Ok(())
    }
}
