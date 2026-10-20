use super::super::{domain, south::adapter::postgres::Repository};
use crate::business::{Actor, Error, Result, require, text};
use crate::identity::south::port::Store;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
#[derive(Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
}
#[derive(Deserialize)]
pub struct NewAccount {
    pub username: String,
    pub display_name: String,
    pub password: String,
}
#[derive(Deserialize)]
pub struct AccountUpdate {
    pub display_name: String,
    pub active: bool,
    pub wecom_userid: Option<String>,
}
impl<R: Store> Service<R> {
    pub async fn login(&self, input: Login) -> Result<(String, Actor)> {
        let name = text(&input.username, 1, 100)?;
        require(input.password.len() <= 128, "密码过长")?;
        self.0.throttle(&name).await?;
        let row = self.0.account(&name).await?;
        let valid = if let Some((_, ref h)) = row {
            let h = h.clone();
            let p = input.password;
            tokio::task::spawn_blocking(move || domain::verify(&p, &h))
                .await
                .map_err(|_| Error::Unavailable)?
        } else {
            false
        };
        if !valid {
            self.0.failed(&name).await?;
            return Err(Error::Unauthorized);
        }
        self.0
            .session(row.expect("verified account").0, "browser", false)
            .await
    }
    pub async fn create(&self, actor: &Actor, input: NewAccount) -> Result<Value> {
        actor.platform()?;
        let id = self
            .bootstrap(&input.username, &input.display_name, &input.password, false)
            .await?;
        Ok(json!({"id":id}))
    }
    pub async fn bootstrap(
        &self,
        username: &str,
        name: &str,
        password: &str,
        admin: bool,
    ) -> Result<Uuid> {
        let hash = domain::password_hash(password)?;
        self.0
            .create(&text(username, 1, 100)?, &text(name, 1, 100)?, &hash, admin)
            .await
    }
    pub async fn list(&self, a: &Actor) -> Result<Vec<Value>> {
        a.platform()?;
        self.0.list().await
    }
    pub async fn update(&self, a: &Actor, id: Uuid, input: AccountUpdate) -> Result<()> {
        a.platform()?;
        require(a.id != id, "不能通过管理接口禁用自身")?;
        self.0
            .update(
                a.id,
                id,
                &text(&input.display_name, 1, 100)?,
                input.active,
                input.wecom_userid.as_deref(),
            )
            .await
    }
    pub async fn reset(&self, a: &Actor, id: Uuid, p: &str) -> Result<()> {
        a.platform()?;
        self.0.reset_password(id, &domain::password_hash(p)?).await
    }
}

impl<R: Store> Service<R> {
    pub async fn authenticate(&self, token: &str, kind: &str) -> Result<Actor> {
        self.0.authenticate(token, kind).await
    }
    pub async fn logout(&self, a: &Actor) -> Result<()> {
        self.0.logout(&a.session_hash).await
    }
    pub async fn profile(&self, a: &Actor, name: &str) -> Result<Value> {
        self.0.profile(a.id, &text(name, 1, 100)?).await
    }
}
