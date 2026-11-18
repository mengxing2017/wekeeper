//! Durable v10 execution transport. Business authorization and persistence stay below HTTP.
use crate::{
    bootstrap::Application,
    business::{Actor, Error, Result},
    distribution::{
        domain::device_control::Credential,
        north::local::shared_send_v2::Service,
        south::{adapter::postgres::shared_send_v2::Repository, port::shared_send_v2::*},
    },
    http::jsonapi::{ApiResponse, Input, resource},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use uuid::Uuid;
fn service(a: &Application) -> Service {
    Service(Repository(a.pool.clone()))
}
fn key(h: &HeaderMap) -> &str {
    h.get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}
async fn credential(a: &Application, h: &HeaderMap) -> Result<Credential> {
    service(a)
        .credential(
            h.get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .ok_or(Error::Unauthorized)?,
        )
        .await
}
async fn list(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a).task_read(&actor, None).await?,
    ))
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a).task_read(&actor, Some(id)).await?,
    ))
}

async fn confirm(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a)
            .task_write(
                &actor,
                key(&h),
                TaskCommand::Confirm(input.decode("mobile-shared-tasks")?),
            )
            .await?,
    ))
}
async fn stop(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a)
            .task_write(
                &actor,
                key(&h),
                TaskCommand::Stop(id, input.decode("mobile-shared-stops")?),
            )
            .await?,
    ))
}
async fn review(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-reviews",
        service(&a)
            .task_write(
                &actor,
                key(&h),
                TaskCommand::Review(id, input.decode("mobile-shared-reviews")?),
            )
            .await?,
    ))
}

async fn inbox(
    State(a): State<Application>,
    h: HeaderMap,
    Query(q): Query<crate::distribution::domain::shared_mobile::shared_send_v2::InboxQuery>,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-task-inboxes",
        service(&a).inbox_read(&c, q).await?,
    ))
}
async fn inbox_receipt(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-task-receipts",
        service(&a)
            .inbox_receipt(&c, key(&h), input.decode("mobile-shared-task-receipts")?)
            .await?,
    ))
}
async fn context(State(a): State<Application>, h: HeaderMap) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-contexts",
        service(&a).device_read(&c).await?,
    ))
}
async fn environment(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-device-environments",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Environment(input.decode("mobile-device-environments")?),
            )
            .await?,
    ))
}

async fn inspection(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-inspections",
        service(&a)
            .task_write(
                &actor,
                key(&h),
                TaskCommand::Inspection(input.decode("mobile-shared-inspections")?),
            )
            .await?,
    ))
}
async fn reassign(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a)
            .task_write(
                &actor,
                key(&h),
                TaskCommand::Reassign(id, input.decode("mobile-shared-reassignments")?),
            )
            .await?,
    ))
}
async fn diagnostic(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-diagnostics",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Diagnostic(input.decode("mobile-shared-diagnostics")?),
            )
            .await?,
    ))
}
async fn inspection_execution(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "inspection-executions",
        service(&a)
            .inspection_execution(&c, key(&h), input.decode("inspection-executions")?)
            .await?,
    ))
}
async fn execution_completion(State(a): State<Application>, input: Input) -> Result<ApiResponse> {
    Ok(resource(
        "execution-completions",
        service(&a)
            .complete_execution(input.decode("execution-completions")?)
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .merge(scheme_router())
        .merge(super::mobile_dispatch_records::router())
        .route(
            "/api/v10/device-execution/inspection-executions",
            post(inspection_execution),
        )
        .route(
            "/api/v10/device-execution/execution-completions",
            post(execution_completion),
        )
        .route("/api/v10/device-execution/task-context", get(context))
        .route("/api/v10/device-execution/task-inbox", get(inbox))
        .route(
            "/api/v10/device-execution/display-receipts",
            post(inbox_receipt),
        )
        .route(
            "/api/v10/device-execution/environment-observations",
            post(environment),
        )
        .route("/api/v10/device-execution/diagnostics", post(diagnostic))
        .route(
            "/api/v1/mobile-execution/shared-v2-items/{id}/reassignments",
            post(reassign),
        )
        .route(
            "/api/v1/mobile-execution/shared-v2-inspections",
            post(inspection),
        )
        .route(
            "/api/v1/mobile-execution/shared-v2-tasks",
            get(list).post(confirm),
        )
        .route("/api/v1/mobile-execution/shared-v2-tasks/{id}", get(detail))
        .route(
            "/api/v1/mobile-execution/shared-v2-tasks/{id}/stops",
            post(stop),
        )
        .route(
            "/api/v1/mobile-execution/shared-v2-items/{id}/reviews",
            post(review),
        )
}

async fn scheme_read_default(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-business-schemes",
        service(&a).scheme_read(&actor, None).await?,
    ))
}
async fn scheme_read_unit(
    State(a): State<Application>,
    actor: Actor,
    Path(unit): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-business-schemes",
        service(&a).scheme_read(&actor, Some(unit)).await?,
    ))
}
async fn save_scheme(
    a: Application,
    actor: Actor,
    h: HeaderMap,
    input: Input,
    unit: Option<Uuid>,
) -> Result<ApiResponse> {
    use crate::distribution::domain::shared_mobile::business_scheme::{
        BusinessSchemeInput, SchemeMutation,
    };
    let v: BusinessSchemeInput = input.decode("mobile-business-schemes")?;
    let mutation = SchemeMutation {
        expected_revision: v.expected_revision,
        reason: v.reason.clone(),
        confirmed: v.confirmed,
    };
    Ok(resource(
        "mobile-business-schemes",
        service(&a)
            .scheme_write(&actor, key(&h), unit, Some(v), mutation)
            .await?,
    ))
}
async fn scheme_save_default(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    save_scheme(a, actor, h, input, None).await
}
async fn scheme_save_unit(
    State(a): State<Application>,
    actor: Actor,
    Path(unit): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    save_scheme(a, actor, h, input, Some(unit)).await
}
async fn scheme_inherit(
    State(a): State<Application>,
    actor: Actor,
    Path(unit): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-business-schemes",
        service(&a)
            .scheme_write(
                &actor,
                key(&h),
                Some(unit),
                None,
                input.decode("mobile-business-schemes")?,
            )
            .await?,
    ))
}
async fn task_preview(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-task-previews",
        service(&a)
            .preview_task(&actor, key(&h), input.decode("mobile-task-previews")?)
            .await?,
    ))
}
fn scheme_router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/mobile-execution/business-schemes/default",
            get(scheme_read_default),
        )
        .route(
            "/api/v1/mobile-execution/business-schemes/default/versions",
            post(scheme_save_default),
        )
        .route(
            "/api/v1/mobile-execution/business-schemes/units/{unit}",
            get(scheme_read_unit),
        )
        .route(
            "/api/v1/mobile-execution/business-schemes/units/{unit}/versions",
            post(scheme_save_unit),
        )
        .route(
            "/api/v1/mobile-execution/business-schemes/units/{unit}/inheritances",
            post(scheme_inherit),
        )
        .route(
            "/api/v1/mobile-execution/shared-v2-task-previews",
            post(task_preview),
        )
}
