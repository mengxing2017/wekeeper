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
                crate::distribution::domain::shared_mobile::usage::UsageCommand::TaskConfirm(
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

async fn delegation_context(State(a): State<Application>, h: HeaderMap) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "mobile-shared-delegation-contexts",
        service(&a).delegation_read(&c).await?,
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
async fn task_delivery(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "task-deliveries",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Deliver(input.decode("task-deliveries")?),
            )
            .await?,
    ))
}
async fn execution_grant(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "execution-grants",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Grant(input.decode("execution-grants")?),
            )
            .await?,
    ))
}
async fn execution_receipt(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = credential(&a, &h).await?;
    Ok(resource(
        "execution-receipts",
        service(&a)
            .device_write(
                &c,
                key(&h),
                DeviceCommand::Receipt(input.decode("execution-receipts")?),
            )
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
        .merge(usage_router())
        .route(
            "/api/v10/device-execution/task-deliveries",
            post(task_delivery),
        )
        .route(
            "/api/v10/device-execution/execution-grants",
            post(execution_grant),
        )
        .route(
            "/api/v10/device-execution/execution-receipts",
            post(execution_receipt),
        )
        .route(
            "/api/v10/device-execution/execution-completions",
            post(execution_completion),
        )
        .route(
            "/api/v10/device-execution/authority-context",
            get(delegation_context),
        )
        .route("/api/v10/device-execution/task-context", get(context))
        .route("/api/v10/device-execution/task-inbox", get(inbox))
        .route(
            "/api/v10/device-execution/display-receipts",
            post(inbox_receipt),
        )
        .route("/api/v10/device-execution/idle-telemetry", post(runtime))
        .route(
            "/api/v10/device-execution/authorization-decisions",
            post(consent),
        )
        .route("/api/v10/device-execution/diagnostics", post(diagnostic))
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-items/{id}/reassignments",
            post(reassign),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/shared-v2-inspections",
            post(inspection),
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
                crate::distribution::domain::shared_mobile::usage::UsageCommand::Preview(
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
                crate::distribution::domain::shared_mobile::usage::UsageCommand::Confirm(
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
                crate::distribution::domain::shared_mobile::usage::UsageCommand::Mutate {
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
                crate::distribution::domain::shared_mobile::usage::UsageCommand::TaskPreview(
                    input.decode("mobile-usage-task-previews")?,
                ),
            )
            .await?,
    ))
}
async fn business_scheme(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let v: crate::distribution::domain::shared_mobile::usage::BusinessSchemeInput =
        input.decode("mobile-business-schemes")?;
    Ok(resource(
        "mobile-business-schemes",
        service(&a)
            .platform_write(&actor, key(&h), PlatformCommand::Policy(v.policy()?))
            .await?,
    ))
}
fn usage_router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/platform/mobile-execution/business-schemes",
            post(business_scheme),
        )
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
                crate::distribution::domain::shared_mobile::usage::UsageCommand::Issue {
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
                crate::distribution::domain::shared_mobile::usage::UsageCommand::IssueUpdate {
                    id,
                    issue_id,
                    input: input.decode("mobile-usage-issues")?,
                },
            )
            .await?,
    ))
}
