use super::super::south::adapter::postgres::Repository;
use crate::business::{Actor, Result};
use crate::distribution::south::port::Store;
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn templates(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        self.0.templates(s).await
    }
}

pub mod mobile_delivery;

pub mod device_control;

pub mod mobile_execution;

pub mod governance;

pub mod shared_send;

pub mod shared_send_v2;

pub mod workflows;
