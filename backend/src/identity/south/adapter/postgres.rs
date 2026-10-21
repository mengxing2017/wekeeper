use crate::business::{Actor, Error, Grant, Result, hash, secret};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Clone)]
pub struct Repository(pub PgPool);
impl Repository {
    pub async fn account(&self, username: &str) -> Result<Option<(Uuid, String)>> {
        let row=sqlx::query("SELECT id,password_hash FROM accounts WHERE username=$1 AND active AND password_hash IS NOT NULL").bind(username).fetch_optional(&self.0).await?;
        Ok(row.map(|r| (r.get("id"), r.get("password_hash"))))
    }
    pub async fn throttle(&self, username: &str) -> Result<()> {
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM login_attempts WHERE username=$1 AND failures>=10 AND reset_at>now())").bind(username).fetch_one(&self.0).await?;
        if blocked {
            Err(Error::Invalid("尝试过多，请 15 分钟后重试".into()))
        } else {
            Ok(())
        }
    }
    pub async fn failed(&self, username: &str) -> Result<()> {
        sqlx::query("INSERT INTO login_attempts VALUES($1,1,now()+interval '15 minutes') ON CONFLICT(username) DO UPDATE SET failures=CASE WHEN login_attempts.reset_at<now() THEN 1 ELSE login_attempts.failures+1 END,reset_at=CASE WHEN login_attempts.reset_at<now() THEN now()+interval '15 minutes' ELSE login_attempts.reset_at END").bind(username).execute(&self.0).await?;
        Ok(())
    }
    pub async fn session(&self, id: Uuid, kind: &str, wecom: bool) -> Result<(String, Actor)> {
        if wecom {
            sqlx::query("SELECT a.id FROM accounts a JOIN wecom_members m ON m.id=a.wecom_member_id WHERE a.id=$1 AND a.active AND m.verification='verified' AND m.status=1").bind(id).fetch_optional(&self.0).await?.ok_or(Error::Forbidden)?;
        }
        let token = secret();
        let csrf = secret();
        sqlx::query("INSERT INTO sessions(token_hash,account_id,csrf,expires_at,kind,wecom_authenticated,wecom_binding_revision) SELECT $1,$2,$3,now()+interval '8 hours',$4,$5,wecom_binding_revision FROM accounts WHERE id=$2").bind(hash(&token)).bind(id).bind(csrf).bind(kind).bind(wecom).execute(&self.0).await?;
        let actor = self.authenticate(&token, kind).await?;
        Ok((token, actor))
    }
    pub async fn authenticate(&self, token: &str, kind: &str) -> Result<Actor> {
        let h = hash(token);
        let row=sqlx::query("UPDATE sessions s SET last_seen_at=now() FROM accounts a WHERE s.account_id=a.id AND s.token_hash=$1 AND s.kind=$2 AND s.expires_at>now() AND s.last_seen_at>now()-interval '30 minutes' AND a.active AND (NOT s.wecom_authenticated OR (s.wecom_binding_revision=a.wecom_binding_revision AND EXISTS(SELECT 1 FROM wecom_members m WHERE m.id=a.wecom_member_id AND m.verification='verified' AND m.status=1))) RETURNING a.id,a.username,a.display_name,a.platform_admin,a.bound_school_id,s.csrf,s.wecom_authenticated,a.wecom_binding_revision").bind(&h).bind(kind).fetch_optional(&self.0).await?.ok_or(Error::Unauthorized)?;
        let id: Uuid = row.get("id");
        let grants=sqlx::query("SELECT m.school_id,m.role FROM school_memberships m JOIN schools s ON s.id=m.school_id WHERE m.account_id=$1 AND s.active AND m.role<>'revoked'").bind(id).fetch_all(&self.0).await?.into_iter().map(|r|Grant{school_id:r.get("school_id"),role:r.get("role")}).collect();
        Ok(Actor {
            id,
            username: row.get("username"),
            display_name: row.get("display_name"),
            platform_admin: row.get("platform_admin"),
            bound_school_id: row.get("bound_school_id"),
            grants,
            csrf: row.get("csrf"),
            session_hash: h,
            request_id: String::new(),
            browser: kind == "browser",
            wecom_authenticated: row.get("wecom_authenticated"),
            wecom_binding_revision: row.get("wecom_binding_revision"),
        })
    }
    pub async fn logout(&self, h: &str) -> Result<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash=$1")
            .bind(h)
            .execute(&self.0)
            .await?;
        Ok(())
    }
    pub async fn create(
        &self,
        username: &str,
        name: &str,
        password: &str,
        admin: bool,
    ) -> Result<Uuid> {
        let id = Uuid::now_v7();
        sqlx::query("INSERT INTO accounts(id,username,display_name,password_hash,platform_admin) VALUES($1,$2,$3,$4,$5)").bind(id).bind(username).bind(name).bind(password).bind(admin).execute(&self.0).await?;
        Ok(id)
    }
    pub async fn list(&self) -> Result<Vec<Value>> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('id',id,'username',username,'display_name',display_name,'active',active,'platform_admin',platform_admin,'bound_school_id',bound_school_id,'wecom_userid',(SELECT userid FROM wecom_members WHERE id=accounts.wecom_member_id),'wecom_member_id',wecom_member_id,'wecom_binding_revision',wecom_binding_revision,'wecom_name',(SELECT name FROM wecom_members WHERE id=accounts.wecom_member_id),'wecom_status',(SELECT status FROM wecom_members WHERE id=accounts.wecom_member_id),'wecom_verification',(SELECT verification FROM wecom_members WHERE id=accounts.wecom_member_id),'wecom_departments',coalesce((SELECT jsonb_agg(d.name ORDER BY d.id) FROM wecom_members m JOIN wecom_departments d ON d.id=ANY(m.department) WHERE m.id=accounts.wecom_member_id),'[]'::jsonb)) FROM accounts ORDER BY created_at DESC,id").fetch_all(&self.0).await?)
    }
    pub async fn update(&self, actor: Uuid, id: Uuid, name: &str, active: bool) -> Result<()> {
        let mut tx = self.0.begin().await?;
        let n = sqlx::query(
            "UPDATE accounts SET display_name=$2,active=$3 WHERE id=$1 AND NOT platform_admin",
        )
        .bind(id)
        .bind(name)
        .bind(active)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if n == 0 {
            return Err(Error::Forbidden);
        }
        sqlx::query("DELETE FROM sessions WHERE account_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO audit_events(id,actor_id,action,object_id) VALUES($1,$2,'account.update',$3)").bind(Uuid::now_v7()).bind(actor).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn reset_password(&self, id: Uuid, password: &str) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("UPDATE accounts SET password_hash=$2 WHERE id=$1")
            .bind(id)
            .bind(password)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM sessions WHERE account_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn profile(&self, id: Uuid, name: &str) -> Result<Value> {
        sqlx::query("UPDATE accounts SET display_name=$2 WHERE id=$1")
            .bind(id)
            .bind(name)
            .execute(&self.0)
            .await?;
        Ok(json!({"display_name":name}))
    }
}

impl crate::identity::south::port::Store for Repository {
    async fn account(&self, username: &str) -> Result<Option<(Uuid, String)>> {
        Repository::account(self, username).await
    }
    async fn throttle(&self, username: &str) -> Result<()> {
        Repository::throttle(self, username).await
    }
    async fn failed(&self, username: &str) -> Result<()> {
        Repository::failed(self, username).await
    }
    async fn session(&self, id: Uuid, kind: &str, wecom: bool) -> Result<(String, Actor)> {
        Repository::session(self, id, kind, wecom).await
    }
    async fn authenticate(&self, token: &str, kind: &str) -> Result<Actor> {
        Repository::authenticate(self, token, kind).await
    }
    async fn logout(&self, h: &str) -> Result<()> {
        Repository::logout(self, h).await
    }
    async fn create(
        &self,
        username: &str,
        name: &str,
        password: &str,
        admin: bool,
    ) -> Result<Uuid> {
        Repository::create(self, username, name, password, admin).await
    }
    async fn list(&self) -> Result<Vec<Value>> {
        Repository::list(self).await
    }
    async fn update(&self, actor: Uuid, id: Uuid, name: &str, active: bool) -> Result<()> {
        Repository::update(self, actor, id, name, active).await
    }
    async fn reset_password(&self, id: Uuid, password: &str) -> Result<()> {
        Repository::reset_password(self, id, password).await
    }
    async fn profile(&self, id: Uuid, name: &str) -> Result<Value> {
        Repository::profile(self, id, name).await
    }
}
