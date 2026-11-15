use super::super::south::adapter::postgres::Repository;
use crate::business::{Actor, Page, Result};
use crate::distribution::south::port::Store;
use serde_json::Value;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn materials(&self, a: &Actor, page: Page) -> Result<Vec<Value>> {
        a.platform()?;
        self.0.materials(page).await
    }

    pub async fn templates(&self, a: &Actor) -> Result<Vec<Value>> {
        a.platform()?;
        self.0.templates().await
    }
}

pub mod device_registry;

pub mod device_control;

pub mod shared_mobile_resources;

pub mod governance;

pub mod shared_send_v2;

pub mod workflows;

pub mod groups;
pub mod receiving;
