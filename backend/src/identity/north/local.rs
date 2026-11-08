use super::super::{domain, south::adapter::postgres::Repository};
use crate::business::{Actor, Error, Result, require, text};
use crate::identity::south::port::Store;
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
#[derive(Deserialize)]
pub struct Login {
    pub username: String,
    pub password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdministratorCreate {
    pub username: String,
    pub display_name: String,
    pub password: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdministratorEdit {
    pub display_name: String,
    pub active: bool,
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
    pub async fn list(&self, a: &Actor, search: &str) -> Result<Vec<Value>> {
        a.platform()?;
        self.0.list(search.trim()).await
    }
    pub async fn create_administrator(
        &self,
        a: &Actor,
        input: AdministratorCreate,
    ) -> Result<Value> {
        a.platform()?;
        let username = text(&input.username, 1, 100)?;
        let name = text(&input.display_name, 1, 100)?;
        let hash = domain::password_hash(&input.password)?;
        self.0
            .create_administrator(a, &username, &name, &hash)
            .await
    }
    pub async fn edit_administrator(
        &self,
        a: &Actor,
        id: Uuid,
        input: AdministratorEdit,
    ) -> Result<Value> {
        a.platform()?;
        require(id != a.id || input.active, "不能停用当前登录管理员")?;
        self.0
            .edit_administrator(a, id, &text(&input.display_name, 1, 100)?, input.active)
            .await
    }
    pub async fn remove_administrator(&self, a: &Actor, id: Uuid) -> Result<()> {
        a.platform()?;
        require(id != a.id, "不能删除当前登录管理员")?;
        self.0.remove_administrator(a, id).await
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
