use crate::message_orchestration::domain::{AcceptMessage, AcceptedMessage, Result};

pub trait MessageStore: Send + Sync {
    fn accept(
        &self,
        message: AcceptMessage,
    ) -> impl std::future::Future<Output = Result<AcceptedMessage>> + Send;
}
