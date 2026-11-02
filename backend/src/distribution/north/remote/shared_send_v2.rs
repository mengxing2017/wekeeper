//! v9 shared-send transport. Business authorization and persistence stay below HTTP.
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
            .usage_write(
                &actor,
                s,
                key(&h),
                crate::distribution::domain::mobile_execution::usage::UsageCommand::TaskConfirm(
                    input.decode("mobile-shared-tasks")?,
                ),
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
    let event: crate::distribution::domain::mobile_execution::shared_send_v2::ResultEvent =
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

async fn settle(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-settlements",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Settle(input.decode("mobile-shared-settlements")?),
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
pub fn router() -> Router<Application> {
    Router::new()
        .merge(usage_router())
        .route("/api/v9/device-execution/send-settlements", post(settle))
        .route(
            "/api/v9/device-execution/failure-diagnostics",
            post(diagnostic),
        )
        .route(
            "/api/v9/device-execution/send-recoveries",
            post(recover_claim),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-items/{id}/reassignments",
            post(reassign),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-inspections",
            post(inspection),
        )
        .route(
            "/api/v9/device-execution/send-inspection-claims",
            post(inspection_claim),
        )
        .route(
            "/api/v9/device-execution/send-inspection-results",
            post(inspection_result),
        )
        .route(
            "/api/v9/device-execution/send-claims/{id}/renewals",
            post(renew),
        )
        .route(
            "/api/v9/device-execution/send-claims/{id}/releases",
            post(release),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-tasks",
            get(list).post(confirm),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-tasks/{id}",
            get(detail),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-tasks/{id}/stops",
            post(stop),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-items/{id}/reviews",
            post(review),
        )
        .route(
            "/api/v9/device-execution/protocol-capabilities",
            get(context),
        )
        .route(
            "/api/v9/device-execution/send-capability-context",
            get(context),
        )
        .route(
            "/api/v9/device-execution/send-recovery-context",
            get(context),
        )
        .route("/api/v9/device-execution/runtime-reports", post(runtime))
        .route(
            "/api/v9/device-execution/send-delegation-decisions",
            post(consent),
        )
        .route("/api/v9/device-execution/send-claims", post(claim))
        .route(
            "/api/v9/device-execution/send-claims/{id}/preparations",
            post(prepare),
        )
        .route(
            "/api/v9/device-execution/send-claims/{id}/invocations",
            post(invoke),
        )
        .route("/api/v9/device-execution/send-results", post(result))
}

async fn usage_options(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usage-options",
        service(&a).usage_read(&actor, Some(s), None, true).await?,
    ))
}
async fn usage_list(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usages",
        service(&a).usage_read(&actor, Some(s), None, false).await?,
    ))
}
async fn usage_global(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usages",
        service(&a).usage_read(&actor, None, None, false).await?,
    ))
}
async fn usage_global_options(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usage-options",
        service(&a).usage_read(&actor, None, None, true).await?,
    ))
}
async fn usage_preview(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usage-previews",
        service(&a)
            .usage_write(
                &actor,
                s,
                key(&h),
                crate::distribution::domain::mobile_execution::usage::UsageCommand::Preview(
                    input.decode("mobile-usage-previews")?,
                ),
            )
            .await?,
    ))
}
async fn usage_confirm(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usages",
        service(&a)
            .usage_write(
                &actor,
                s,
                key(&h),
                crate::distribution::domain::mobile_execution::usage::UsageCommand::Confirm(
                    input.decode("mobile-usages")?,
                ),
            )
            .await?,
    ))
}
async fn usage_mutate(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id, operation)): Path<(Uuid, Uuid, String)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usages",
        service(&a)
            .usage_write(
                &actor,
                s,
                key(&h),
                crate::distribution::domain::mobile_execution::usage::UsageCommand::Mutate {
                    id,
                    operation,
                    mutation: input.decode("mobile-usages")?,
                },
            )
            .await?,
    ))
}
async fn usage_task_preview(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usage-task-previews",
        service(&a)
            .usage_write(
                &actor,
                s,
                key(&h),
                crate::distribution::domain::mobile_execution::usage::UsageCommand::TaskPreview(
                    input.decode("mobile-usage-task-previews")?,
                ),
            )
            .await?,
    ))
}
fn usage_router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/schools/{school}/mobile-execution/usages/{id}/issues/{issue}/responses",
            post(usage_issue_update),
        )
        .route(
            "/api/v1/platform/mobile-execution/usage-requests",
            get(usage_global),
        )
        .route(
            "/api/v1/platform/mobile-execution/usage-options",
            get(usage_global_options),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/usage-options",
            get(usage_options),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/usage-previews",
            post(usage_preview),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/usages",
            get(usage_list).post(usage_confirm),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/usages/{id}",
            get(usage_detail),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/usages/{id}/issues",
            post(usage_issue),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/usages/{id}/{operation}",
            post(usage_mutate),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/usage-task-previews",
            post(usage_task_preview),
        )
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct UsageIssue {
    description: String,
}
async fn usage_issue(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let v: UsageIssue = input.decode("mobile-usage-issues")?;
    Ok(resource(
        "mobile-usage-issues",
        service(&a)
            .usage_write(
                &actor,
                s,
                key(&h),
                crate::distribution::domain::mobile_execution::usage::UsageCommand::Issue {
                    id,
                    description: v.description,
                },
            )
            .await?,
    ))
}
async fn usage_detail(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usages",
        service(&a)
            .usage_read(&actor, Some(s), Some(id), false)
            .await?,
    ))
}

async fn usage_issue_update(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id, issue_id)): Path<(Uuid, Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-usages",
        service(&a)
            .usage_write(
                &actor,
                s,
                key(&h),
                crate::distribution::domain::mobile_execution::usage::UsageCommand::IssueUpdate {
                    id,
                    issue_id,
                    input: input.decode("mobile-usage-issues")?,
                },
            )
            .await?,
    ))
}
