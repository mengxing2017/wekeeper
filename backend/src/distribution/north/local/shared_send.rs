use crate::{
    business::{Actor, Result, text},
    distribution::{
        domain::device_control::Credential,
        south::{adapter::postgres::shared_send::Repository, port::shared_send::*},
    },
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn recover_claim(
        &self,
        event: crate::distribution::domain::mobile_execution::shared_send::ClaimRecovery,
    ) -> Result<Value> {
        self.0.recover_claim(event).await
    }

    pub async fn recover_result(
        &self,
        key: &str,
        event: crate::distribution::domain::mobile_execution::shared_send::ResultEvent,
    ) -> Result<Value> {
        self.0.recover_result(&text(key, 1, 128)?, event).await
    }

    pub async fn school_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: SchoolCommand,
    ) -> Result<Value> {
        a.authorize(
            s,
            if matches!(
                &command,
                SchoolCommand::Draft(..) | SchoolCommand::Preview(_)
            ) {
                "operator"
            } else {
                "admin"
            },
        )?;
        self.0
            .school_write(a, s, &text(key, 1, 128)?, command)
            .await
    }
    pub async fn school_read(&self, a: &Actor, s: Uuid, id: Option<Uuid>) -> Result<Value> {
        a.authorize(s, "operator")?;
        self.0.school_read(a, s, id).await
    }
    pub async fn platform_write(
        &self,
        a: &Actor,
        key: &str,
        command: PlatformCommand,
    ) -> Result<Value> {
        a.platform()?;
        self.0.platform_write(a, &text(key, 1, 128)?, command).await
    }
    pub async fn platform_read(&self, a: &Actor) -> Result<Value> {
        a.platform()?;
        self.0.platform_read(a).await
    }
    pub async fn device_write(
        &self,
        c: &Credential,
        key: &str,
        command: DeviceCommand,
    ) -> Result<Value> {
        self.0.device_write(c, &text(key, 1, 128)?, command).await
    }
    pub async fn device_read(&self, c: &Credential) -> Result<Value> {
        self.0.device_read(c).await
    }
}
