use crate::{
    business::{Actor, Result, text},
    distribution::{
        domain::device_control::Credential,
        south::{adapter::postgres::shared_send_v2::Repository, port::shared_send_v2::*},
    },
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn complete_execution(
        &self,
        v: crate::distribution::domain::shared_mobile::execution::Completion,
    ) -> Result<Value> {
        v.validate()?;
        self.0.complete_execution(v).await
    }

    pub async fn inbox_read(
        &self,
        c: &Credential,
        q: crate::distribution::domain::shared_mobile::shared_send_v2::InboxQuery,
    ) -> Result<Value> {
        self.0.inbox_read(c, q).await
    }
    pub async fn inbox_receipt(
        &self,
        c: &Credential,
        key: &str,
        v: crate::distribution::domain::shared_mobile::shared_send_v2::InboxReceipt,
    ) -> Result<Value> {
        self.0.inbox_receipt(c, &text(key, 1, 128)?, v).await
    }
    pub async fn credential(&self, token: &str) -> Result<Credential> {
        self.0.credential(token).await
    }
    pub async fn usage_read(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        id: Option<Uuid>,
        get_options: bool,
    ) -> Result<Value> {
        self.0.usage_read(a, s, id, get_options).await
    }
    pub async fn usage_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: crate::distribution::domain::shared_mobile::usage::UsageCommand,
    ) -> Result<Value> {
        self.0.usage_write(a, s, &text(key, 1, 128)?, command).await
    }
    pub async fn school_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: SchoolCommand,
    ) -> Result<Value> {
        self.0
            .school_write(a, s, &text(key, 1, 128)?, command)
            .await
    }
    pub async fn school_read(&self, a: &Actor, s: Uuid, id: Option<Uuid>) -> Result<Value> {
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
    pub async fn platform_read(&self, a: &Actor, page: u32) -> Result<Value> {
        a.platform()?;
        self.0.platform_read(a, page).await
    }
    pub async fn artifact_content(&self, a: &Actor, id: Uuid) -> Result<Vec<u8>> {
        a.platform()?;
        self.0.artifact_content(a, id).await
    }
    pub async fn device_write(
        &self,
        c: &Credential,
        key: &str,
        command: DeviceCommand,
    ) -> Result<Value> {
        self.0.device_write(c, &text(key, 1, 128)?, command).await
    }
    pub async fn delegation_read(&self, c: &Credential) -> Result<Value> {
        self.0.delegation_read(c).await
    }
    pub async fn device_read(&self, c: &Credential) -> Result<Value> {
        self.0.device_read(c).await
    }
}
