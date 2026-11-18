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
    pub fn delivery_notifications(&self) -> tokio::sync::broadcast::Receiver<Uuid> {
        Repository::subscribe_delivery_notifications()
    }
    pub async fn pending_deliveries(
        &self,
        c: &Credential,
        connection: Uuid,
        epoch: i64,
    ) -> Result<Vec<crate::distribution::domain::shared_mobile::execution::ExecutionDelivery>> {
        self.0.pending_deliveries(c, connection, epoch).await
    }
    pub async fn delivery_written(&self, id: Uuid, completed: bool) -> Result<()> {
        self.0.delivery_written(id, completed).await
    }
    pub async fn inspection_execution(
        &self,
        c: &Credential,
        key: &str,
        v: crate::distribution::domain::shared_mobile::execution::InspectionRequest,
    ) -> Result<Value> {
        self.0.inspection_execution(c, &text(key, 1, 128)?, v).await
    }

    pub async fn submit_mobile(
        &self,
        message: Uuid,
        receipt: Uuid,
        plan: crate::distribution::domain::receiving::DeliveryPlan,
        content: String,
    ) -> Result<Uuid> {
        self.0.submit_mobile(message, receipt, plan, content).await
    }
    pub async fn records_read(
        &self,
        actor: &Actor,
        kind: &str,
        id: Option<Uuid>,
        query: crate::distribution::domain::shared_mobile::dispatch_records::RecordQuery,
    ) -> Result<Value> {
        actor.platform()?;
        query.limit()?;
        self.0.records_read(actor, kind, id, query).await
    }

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
    pub async fn scheme_read(&self, a: &Actor, unit: Option<Uuid>) -> Result<Value> {
        self.0.scheme_read(a, unit).await
    }
    pub async fn scheme_write(
        &self,
        a: &Actor,
        key: &str,
        unit: Option<Uuid>,
        input: Option<
            crate::distribution::domain::shared_mobile::business_scheme::BusinessSchemeInput,
        >,
        mutation: crate::distribution::domain::shared_mobile::business_scheme::SchemeMutation,
    ) -> Result<Value> {
        self.0
            .scheme_write(a, &text(key, 1, 128)?, unit, input, mutation)
            .await
    }
    pub async fn preview_task(
        &self,
        a: &Actor,
        key: &str,
        input: crate::distribution::domain::shared_mobile::business_scheme::TaskPreviewInput,
    ) -> Result<Value> {
        self.0.preview_task(a, &text(key, 1, 128)?, input).await
    }
    pub async fn task_write(&self, a: &Actor, key: &str, command: TaskCommand) -> Result<Value> {
        self.0.task_write(a, &text(key, 1, 128)?, command).await
    }
    pub async fn task_read(&self, a: &Actor, id: Option<Uuid>) -> Result<Value> {
        self.0.task_read(a, id).await
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

    pub async fn device_read(&self, c: &Credential) -> Result<Value> {
        self.0.device_read(c).await
    }
}
