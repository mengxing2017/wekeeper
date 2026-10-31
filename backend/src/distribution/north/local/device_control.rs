use crate::{
    business::{Error, Result, hash, text},
    distribution::{
        domain::{device_control::*, mobile_delivery::Principal},
        south::{adapter::postgres::device_control::Repository, port::device_control::Store},
    },
};
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn device(&self, c: &Credential, op: Operation) -> Result<Frame> {
        if let Operation::Pulse(p) = &op {
            text(&p.readiness, 1, 80)?;
            if let Some(reason) = &p.block_reason {
                text(reason, 0, 120)?;
            }
            if p.receipts.len() > 100 {
                return Err(Error::Invalid("回执数量超限".into()));
            }
            for r in &p.receipts {
                if let Some(code) = &r.result_code {
                    text(code, 0, 80)?;
                }
            }
        }
        self.0.device(c, op).await
    }
    pub async fn presence(&self, actor: Uuid, device: Uuid) -> Result<serde_json::Value> {
        self.0.presence(actor, device).await
    }
    pub async fn enqueue(
        &self,
        actor: Uuid,
        device: Uuid,
        key: &str,
        input: CommandInput,
    ) -> Result<serde_json::Value> {
        let key = text(key, 1, 128)?;
        self.0.enqueue(actor, device, &key, input).await
    }
}
impl crate::bootstrap::Application {
    pub fn device_control(&self) -> Service {
        Service(Repository(self.pool.clone()))
    }
    pub async fn control_credential(&self, token: &str) -> Result<Credential> {
        if !token.starts_with("wk_device_") {
            return Err(Error::Unauthorized);
        }
        match self.mobile_delivery().authenticate(token).await? {
            Principal::Device { id, generation } => Ok(Credential {
                device_id: id,
                generation,
                token_hash: hash(token),
            }),
            _ => Err(Error::Unauthorized),
        }
    }
}
