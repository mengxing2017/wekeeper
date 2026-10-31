use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    distribution::south::port::mobile_execution::Resource,
    http::jsonapi::{ApiResponse, Input, collection, resource},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;
#[derive(Deserialize, Default)]
struct Filter {
    #[serde(flatten)]
    page: Page,
    #[serde(default)]
    search: String,
    device_id: Option<Uuid>,
}
fn key(h: &HeaderMap) -> &str {
    h.get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}
async fn platform_devices(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-devices",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::PlatformDevices,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn targets(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-targets",
        a.mobile_execution()
            .list(
                &actor,
                Some(s),
                Resource::Targets,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn devices(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-devices",
        a.mobile_execution()
            .list(
                &actor,
                Some(s),
                Resource::Devices,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn bindings(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-target-bindings",
        a.mobile_execution()
            .list(
                &actor,
                Some(s),
                Resource::Bindings,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn grants(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-device-school-grants",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::Grants,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn target(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-targets",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "target",
                None,
                key(&h),
                input.decode::<Value>("mobile-targets")?,
            )
            .await?,
    ))
}
async fn binding(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-target-bindings",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "bind",
                None,
                key(&h),
                input.decode::<Value>("mobile-target-bindings")?,
            )
            .await?,
    ))
}
async fn grant(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-device-school-grants",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "grant",
                None,
                key(&h),
                input.decode::<Value>("mobile-device-school-grants")?,
            )
            .await?,
    ))
}
async fn revoke_grant(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-device-school-grants",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "revoke-grant",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-device-school-grants")?,
            )
            .await?,
    ))
}
async fn revoke_binding(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-target-bindings",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "revoke-binding",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-target-bindings")?,
            )
            .await?,
    ))
}
async fn target_detail(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-targets",
        a.mobile_execution().target(&actor, s, id).await?,
    ))
}
async fn update_target(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-targets",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "update-target",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-targets")?,
            )
            .await?,
    ))
}
async fn archive_target(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-targets",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "archive-target",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-targets")?,
            )
            .await?,
    ))
}
async fn device_credential(
    a: &Application,
    h: &HeaderMap,
) -> Result<crate::distribution::domain::device_control::Credential> {
    let token = h
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(crate::business::Error::Unauthorized)?;
    a.control_credential(token).await
}
async fn consent_context(State(a): State<Application>, h: HeaderMap) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-device-consent-contexts",
        serde_json::json!({"id":c.device_id,"protocol_version":6,"scopes":a.mobile_execution().consent_context(&c).await?}),
    ))
}
async fn consent(State(a): State<Application>, h: HeaderMap, input: Input) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-device-consents",
        a.mobile_execution()
            .consent(&c, key(&h), input.decode("mobile-device-consents")?)
            .await?,
    ))
}
async fn flows(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-execution-flow-versions",
        a.mobile_execution()
            .list(
                &actor,
                Some(s),
                Resource::Flows,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn create_flows(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-execution-flow-versions",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "flow",
                None,
                key(&h),
                input.decode::<Value>("mobile-execution-flow-versions")?,
            )
            .await?,
    ))
}
async fn flow_validations(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-execution-flow-validations",
        a.mobile_execution()
            .list(
                &actor,
                Some(s),
                Resource::Validations,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn create_flow_validations(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-execution-flow-validations",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "validation",
                None,
                key(&h),
                input.decode::<Value>("mobile-execution-flow-validations")?,
            )
            .await?,
    ))
}
async fn releases(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-execution-flow-releases",
        a.mobile_execution()
            .list(
                &actor,
                Some(s),
                Resource::Releases,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn create_releases(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-execution-flow-releases",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "release",
                None,
                key(&h),
                input.decode::<Value>("mobile-execution-flow-releases")?,
            )
            .await?,
    ))
}
async fn assignments(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-execution-assignments",
        a.mobile_execution()
            .list(
                &actor,
                Some(s),
                Resource::Assignments,
                f.page.clone(),
                f.search,
                f.device_id,
            )
            .await?,
        &f.page,
    ))
}
async fn create_assignments(
    State(a): State<Application>,
    actor: Actor,
    Path(s): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-execution-assignments",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "assignment",
                None,
                key(&h),
                input.decode::<Value>("mobile-execution-assignments")?,
            )
            .await?,
    ))
}
async fn revoke_release(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-execution-flow-releases",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "revoke-release",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-execution-flow-releases")?,
            )
            .await?,
    ))
}
async fn update_assignment(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-execution-assignments",
        a.mobile_execution()
            .write(
                &actor,
                Some(s),
                "update-assignment",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-execution-assignments")?,
            )
            .await?,
    ))
}
async fn device_validations(State(a): State<Application>, h: HeaderMap) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(collection(
        "mobile-execution-flow-validations",
        a.mobile_execution().validations(&c).await?,
        &Page::default(),
    ))
}
async fn device_validation_result(
    State(a): State<Application>,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-execution-flow-validations",
        a.mobile_execution()
            .validation_result(
                &c,
                id,
                key(&h),
                input.decode("mobile-execution-flow-validations")?,
            )
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v6/device-execution/flow-validations",
            get(device_validations),
        )
        .route(
            "/api/v6/device-execution/flow-validations/{id}/results",
            post(device_validation_result),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/flow-versions",
            get(flows).post(create_flows),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/flow-validations",
            get(flow_validations).post(create_flow_validations),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/flow-releases",
            get(releases).post(create_releases),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/flow-releases/{id}/revocations",
            post(revoke_release),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/assignments",
            get(assignments).post(create_assignments),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/assignments/{id}",
            axum::routing::patch(update_assignment),
        )
        .route(
            "/api/v6/device-execution/authorization-context",
            get(consent_context),
        )
        .route("/api/v6/device-execution/consents", post(consent))
        .route(
            "/api/v1/platform/mobile-execution/devices",
            get(platform_devices),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/targets/{id}",
            get(target_detail)
                .patch(update_target)
                .delete(archive_target),
        )
        .route(
            "/api/v1/platform/mobile-execution/device-school-grants",
            get(grants).post(grant),
        )
        .route(
            "/api/v1/platform/mobile-execution/device-school-grants/{id}/revocations",
            post(revoke_grant),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/devices",
            get(devices),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/targets",
            get(targets).post(target),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/targets-with-bindings",
            post(target),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/target-bindings",
            get(bindings).post(binding),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/target-bindings/{id}/revocations",
            post(revoke_binding),
        )
}
