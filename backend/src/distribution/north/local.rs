use super::super::south::adapter::postgres::Repository;
use crate::business::{Actor, Page, Result};
use crate::distribution::south::port::Store;
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn materials(&self, a: &Actor, s: Uuid, page: Page) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        self.0.materials(s, page).await
    }

    pub async fn templates(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        self.0.templates(s).await
    }
}

pub mod device_registry;

pub mod device_control;

pub mod shared_mobile_resources;

pub mod governance;

pub mod shared_send_v2;

pub mod workflows;

pub mod mobile_workspaces;

pub mod wecom_workspaces;

pub mod groups;
