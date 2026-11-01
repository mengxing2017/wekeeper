//! v8 shared-send transport. Business authorization and persistence stay below HTTP.
use crate::{
    bootstrap::Application,
    business::{Actor, Error, Result},
    distribution::{
        domain::device_control::Credential,
        north::local::shared_send::Service,
        south::{adapter::postgres::shared_send::Repository, port::shared_send::*},
    },
    http::jsonapi::{ApiResponse, Input, resource},
};
use axum::{
    Router,
    extract::{Path, State},
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
    a.control_credential(
        h.get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(Error::Unauthorized)?,
    )
    .await
}
async fn list(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a).school_read(&actor, s, None).await?,
    ))
}
async fn detail(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a).school_read(&actor, s, Some(id)).await?,
    ))
}
async fn draft(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-drafts",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Draft(None, input.decode("mobile-shared-drafts")?),
            )
            .await?,
    ))
}
async fn edit(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-drafts",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Draft(Some(id), input.decode("mobile-shared-drafts")?),
            )
            .await?,
    ))
}
async fn preview(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-previews",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Preview(input.decode("mobile-shared-previews")?),
            )
            .await?,
    ))
}
async fn confirm(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Confirm(input.decode("mobile-shared-tasks")?),
            )
            .await?,
    ))
}
async fn stop(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Stop(id, input.decode("mobile-shared-stops")?),
            )
            .await?,
    ))
}
async fn review(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-reviews",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Review(id, input.decode("mobile-shared-reviews")?),
            )
            .await?,
    ))
}
async fn platform(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-runtime",
        service(&a).platform_read(&actor).await?,
    ))
}
async fn policy(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-policies",
        service(&a)
            .platform_write(
                &actor,
                key(&h),
                PlatformCommand::Policy(input.decode("mobile-shared-policies")?),
            )
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
async fn runtime(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-runtime",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Runtime(input.decode("mobile-shared-runtime")?),
            )
            .await?,
    ))
}
async fn claim(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-claims",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Claim(input.decode("mobile-shared-claims")?),
            )
            .await?,
    ))
}
async fn prepare(
    State(a): State<Application>,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-preparations",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Prepare(id, input.decode("mobile-shared-preparations")?),
            )
            .await?,
    ))
}
async fn invoke(
    State(a): State<Application>,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-invocations",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Invoke(id, input.decode("mobile-shared-invocations")?),
            )
            .await?,
    ))
}
async fn result(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<ApiResponse> {
    let event: crate::distribution::domain::mobile_execution::shared_send::ResultEvent =
        input.decode("mobile-shared-results")?;
    // The credential is bound to exactly one invocation; it grants no reads, claims or UI authority.
    let value = if event.invocation_id.is_some() {
        service(&a).recover_result(key(&h), event).await?
    } else {
        let c = credential(&a, &h).await?;
        service(&a)
            .device_write(&c, key(&h), DeviceCommand::Result(event))
            .await?
    };
    Ok(resource("mobile-shared-results", value))
}
async fn consent(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-delegation-decisions",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Consent(input.decode("mobile-shared-delegation-decisions")?),
            )
            .await?,
    ))
}

async fn pool_binding(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-bindings",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::PoolBinding(input.decode("mobile-shared-bindings")?),
            )
            .await?,
    ))
}
async fn pool_unbind(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-bindings",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Unbind(id, input.decode("mobile-shared-bindings")?),
            )
            .await?,
    ))
}
async fn school_access(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-access",
        service(&a)
            .platform_write(
                &actor,
                key(&h),
                PlatformCommand::Access(input.decode("mobile-shared-access")?),
            )
            .await?,
    ))
}
async fn inspection(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-inspections",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Inspection(input.decode("mobile-shared-inspections")?),
            )
            .await?,
    ))
}
async fn inspection_claim(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-inspection-claims",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::InspectionClaim(input.decode("mobile-shared-inspection-claims")?),
            )
            .await?,
    ))
}
async fn inspection_result(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-inspection-results",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::InspectionResult(input.decode("mobile-shared-inspection-results")?),
            )
            .await?,
    ))
}
async fn renew(
    State(a): State<Application>,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-leases",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Renew(id, input.decode("mobile-shared-leases")?),
            )
            .await?,
    ))
}
async fn release(
    State(a): State<Application>,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-leases",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Release(id, input.decode("mobile-shared-leases")?),
            )
            .await?,
    ))
}
async fn reassign(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-tasks",
        service(&a)
            .school_write(
                &actor,
                s,
                key(&h),
                SchoolCommand::Reassign(id, input.decode("mobile-shared-reassignments")?),
            )
            .await?,
    ))
}
async fn recover_claim(State(a): State<Application>, input: Input) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-shared-recoveries",
        service(&a)
            .recover_claim(input.decode("mobile-shared-recoveries")?)
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/platform/mobile-execution/shared-device-access",
            post(school_access),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-bindings",
            post(pool_binding),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-bindings/{id}/revocations",
            post(pool_unbind),
        )
        .route(
            "/api/v8/device-execution/send-recoveries",
            post(recover_claim),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-items/{id}/reassignments",
            post(reassign),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-inspections",
            post(inspection),
        )
        .route(
            "/api/v8/device-execution/send-inspection-claims",
            post(inspection_claim),
        )
        .route(
            "/api/v8/device-execution/send-inspection-results",
            post(inspection_result),
        )
        .route(
            "/api/v8/device-execution/send-claims/{id}/renewals",
            post(renew),
        )
        .route(
            "/api/v8/device-execution/send-claims/{id}/releases",
            post(release),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-task-drafts",
            post(draft),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-task-drafts/{id}",
            axum::routing::patch(edit),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-task-previews",
            post(preview),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-tasks",
            get(list).post(confirm),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-tasks/{id}",
            get(detail),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-tasks/{id}/stops",
            post(stop),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-items/{id}/reviews",
            post(review),
        )
        .route(
            "/api/v1/platform/mobile-execution/shared-readiness",
            get(platform),
        )
        .route(
            "/api/v1/platform/mobile-execution/shared-send-policies",
            post(policy),
        )
        .route(
            "/api/v8/device-execution/protocol-capabilities",
            get(context),
        )
        .route(
            "/api/v8/device-execution/send-capability-context",
            get(context),
        )
        .route(
            "/api/v8/device-execution/send-recovery-context",
            get(context),
        )
        .route("/api/v8/device-execution/runtime-reports", post(runtime))
        .route(
            "/api/v8/device-execution/send-delegation-decisions",
            post(consent),
        )
        .route("/api/v8/device-execution/send-claims", post(claim))
        .route(
            "/api/v8/device-execution/send-claims/{id}/preparations",
            post(prepare),
        )
        .route(
            "/api/v8/device-execution/send-claims/{id}/invocations",
            post(invoke),
        )
        .route("/api/v8/device-execution/send-results", post(result))
}
