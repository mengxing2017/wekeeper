use crate::{
    business::{Actor, Result},
    distribution::domain::{device_control::Credential, shared_mobile::shared_send_v2::*},
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone, Debug)]
pub enum TaskCommand {
    Draft(Option<Uuid>, DraftInput),
    Inspection(InspectionInput),
    Preview(PreviewInput),
    Confirm(Confirmation),
    Stop(Uuid, Mutation),
    Review(Uuid, Review),
    Reassign(Uuid, Reassignment),
}
#[derive(Clone, Debug)]
pub enum DeviceCommand {
    Environment(EnvironmentObservation),
    Diagnostic(Diagnostic),
    Inspection(crate::distribution::domain::shared_mobile::execution::InspectionRequest),
}
pub trait Store: Clone + Send + Sync {
    fn pending_deliveries(
        &self,
        c: &Credential,
        connection: Uuid,
        epoch: i64,
    ) -> impl Future<
        Output = Result<
            Vec<crate::distribution::domain::shared_mobile::execution::ExecutionDelivery>,
        >,
    > + Send;
    fn delivery_written(
        &self,
        id: Uuid,
        completed: bool,
    ) -> impl Future<Output = Result<()>> + Send;
    fn inspection_execution(
        &self,
        c: &Credential,
        key: &str,
        v: crate::distribution::domain::shared_mobile::execution::InspectionRequest,
    ) -> impl Future<Output = Result<Value>> + Send;

    fn submit_mobile(
        &self,
        message: Uuid,
        receipt: Uuid,
        plan: crate::distribution::domain::receiving::DeliveryPlan,
        content: String,
    ) -> impl Future<Output = Result<Uuid>> + Send;
    fn records_read(
        &self,
        actor: &Actor,
        kind: &str,
        id: Option<Uuid>,
        query: crate::distribution::domain::shared_mobile::dispatch_records::RecordQuery,
    ) -> impl Future<Output = Result<Value>> + Send;

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
    fn scheme_read(
        &self,
        a: &Actor,
        unit: Option<Uuid>,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn scheme_write(
        &self,
        a: &Actor,
        key: &str,
        unit: Option<Uuid>,
        input: Option<
            crate::distribution::domain::shared_mobile::business_scheme::BusinessSchemeInput,
        >,
        mutation: crate::distribution::domain::shared_mobile::business_scheme::SchemeMutation,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn preview_task(
        &self,
        a: &Actor,
        key: &str,
        input: crate::distribution::domain::shared_mobile::business_scheme::TaskPreviewInput,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn task_write(
        &self,
        a: &Actor,
        key: &str,
        command: TaskCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn task_read(&self, a: &Actor, id: Option<Uuid>) -> impl Future<Output = Result<Value>> + Send;
    fn device_write(
        &self,
        c: &Credential,
        key: &str,
        command: DeviceCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
    fn device_read(&self, c: &Credential) -> impl Future<Output = Result<Value>> + Send;
}
