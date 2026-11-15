use crate::{
    business::{Actor, Result},
    distribution::domain::{device_control::Credential, shared_mobile::shared_send_v2::*},
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone, Debug)]
pub enum TaskCommand {
    Support(SupportInput),

    Draft(Option<Uuid>, DraftInput),
    Inspection(InspectionInput),
    Preview(PreviewInput),
    Confirm(Confirmation),
    Stop(Uuid, Mutation),
    Review(Uuid, Review),
    Reassign(Uuid, Reassignment),
}
#[derive(Clone, Debug)]
pub enum PlatformCommand {
    Policy(PolicyInput),
    Manage(ManagementCommand),
    Artifact(ArtifactUpload),
}
#[derive(Clone, Debug)]
pub enum DeviceCommand {
    Runtime(RuntimeReport),
    Diagnostic(Diagnostic),
    Consent(ConsentInput),
    Deliver(crate::distribution::domain::shared_mobile::execution::DeliveryRequest),
    Grant(crate::distribution::domain::shared_mobile::execution::GrantRequest),
    Receipt(crate::distribution::domain::shared_mobile::execution::ExecutionReceipt),
}
pub trait Store: Clone + Send + Sync {
    fn complete_execution(
        &self,
        v: crate::distribution::domain::shared_mobile::execution::Completion,
    ) -> impl Future<Output = Result<Value>> + Send;

    fn inbox_read(
        &self,
        c: &Credential,
        q: InboxQuery,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn inbox_receipt(
        &self,
        c: &Credential,
        key: &str,
        v: InboxReceipt,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn credential(&self, token: &str) -> impl Future<Output = Result<Credential>> + Send;
    fn usage_read(
        &self,
        a: &Actor,
        id: Option<Uuid>,
        get_options: bool,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn usage_write(
        &self,
        a: &Actor,
        key: &str,
        command: crate::distribution::domain::shared_mobile::usage::UsageCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn task_write(
        &self,
        a: &Actor,
        key: &str,
        command: TaskCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn task_read(&self, a: &Actor, id: Option<Uuid>) -> impl Future<Output = Result<Value>> + Send;
    fn platform_write(
        &self,
        a: &Actor,
        key: &str,
        command: PlatformCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn platform_read(&self, a: &Actor, page: u32) -> impl Future<Output = Result<Value>> + Send;
    fn artifact_content(&self, a: &Actor, id: Uuid)
    -> impl Future<Output = Result<Vec<u8>>> + Send;
    fn device_write(
        &self,
        c: &Credential,
        key: &str,
        command: DeviceCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn delegation_read(&self, c: &Credential) -> impl Future<Output = Result<Value>> + Send;
    fn device_read(&self, c: &Credential) -> impl Future<Output = Result<Value>> + Send;
}
