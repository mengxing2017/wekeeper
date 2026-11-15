use crate::business::{Page, Result};
use serde_json::Value;
pub trait Store: Clone + Send + Sync + 'static {
    fn materials(&self, page: Page)
    -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
    fn templates(&self) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
}

pub mod broadcast;

pub mod device_registry;

pub mod device_control;

pub mod shared_mobile_resources;

pub mod content_source;

pub mod shared_send_v2;

pub mod workflows;

pub mod receiving;
