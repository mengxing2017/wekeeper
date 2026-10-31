use crate::{
    business::Result,
    distribution::domain::device_control::{CommandInput, Credential, Frame, Operation},
};
use serde_json::Value;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn device(
        &self,
        credential: &Credential,
        op: Operation,
    ) -> impl std::future::Future<Output = Result<Frame>> + Send;
    fn presence(
        &self,
        actor: Uuid,
        device: Uuid,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
    fn enqueue(
        &self,
        actor: Uuid,
        device: Uuid,
        key: &str,
        input: CommandInput,
    ) -> impl std::future::Future<Output = Result<Value>> + Send;
}
