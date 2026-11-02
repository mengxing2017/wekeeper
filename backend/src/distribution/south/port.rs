use crate::business::Result;
use serde_json::Value;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn templates(&self, s: Uuid) -> impl std::future::Future<Output = Result<Vec<Value>>> + Send;
}

pub mod broadcast;

pub mod mobile_delivery;

pub mod device_control;

pub mod mobile_execution;

pub mod content_source;

pub mod shared_send;

pub mod shared_send_v2;

pub mod workflows;
