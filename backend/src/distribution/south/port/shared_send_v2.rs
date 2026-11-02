use crate::{
    business::{Actor, Result},
    distribution::domain::{device_control::Credential, mobile_execution::shared_send_v2::*},
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone, Debug)]
pub enum SchoolCommand {
    Support(SupportInput),
    PoolBinding(PoolBindingInput),
    Unbind(Uuid, Mutation),
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
    Access(SchoolAccessInput),
}
#[derive(Clone, Debug)]
pub enum DeviceCommand {
    Runtime(RuntimeReport),
    Settle(Settlement),
    HumanSample(HumanSampleEvent),
    Diagnostic(Diagnostic),
    Claim(ClaimRequest),
    Prepare(Uuid, Preparation),
    Invoke(Uuid, InvocationInput),
    Result(ResultEvent),
    Consent(ConsentInput),
    InspectionClaim(InspectionClaim),
    InspectionResult(InspectionResult),
    Renew(Uuid, LeaseMutation),
    Release(Uuid, LeaseMutation),
}
pub trait Store: Clone + Send + Sync {
    fn usage_read(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        id: Option<Uuid>,
        get_options: bool,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn usage_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: crate::distribution::domain::mobile_execution::usage::UsageCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn recover_claim(&self, event: ClaimRecovery) -> impl Future<Output = Result<Value>> + Send;
    fn recover_result(
        &self,
        key: &str,
        event: ResultEvent,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn school_write(
        &self,
        a: &Actor,
        s: Uuid,
        key: &str,
        command: SchoolCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn school_read(
        &self,
        a: &Actor,
        s: Uuid,
        id: Option<Uuid>,
    ) -> impl Future<Output = Result<Value>> + Send;
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
    fn device_read(&self, c: &Credential) -> impl Future<Output = Result<Value>> + Send;
}
