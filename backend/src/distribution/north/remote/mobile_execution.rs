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
// Keep query fields flat: serde's flatten buffers URL values as strings,
// preventing numeric pagination fields from using the URL deserializer.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct Filter {
    #[serde(rename = "page[number]")]
    page_number: Option<i64>,
    #[serde(rename = "page[size]")]
    page_size: Option<i64>,
    #[serde(default)]
    search: String,
    device_id: Option<Uuid>,
    status: Option<String>,
    mine: Option<bool>,
    kind: Option<String>,
}
impl Filter {
    fn page(&self) -> Page {
        Page {
            page: self.page_number,
            page_size: self.page_size,
            status: self.status.clone(),
            mine: self.mine,
            kind: self.kind.clone(),
            ..Page::default()
        }
    }
}
fn key(h: &HeaderMap) -> &str {
    h.get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
}
async fn platform_openings(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-capability-openings",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::Openings,
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
    ))
}
async fn adapter_releases(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-adapter-releases",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::AdapterReleases,
                f.page(),
                f.search.clone(),
                None,
            )
            .await?,
        &f.page(),
    ))
}
async fn adapter_support(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-adapter-support-requests",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::AdapterSupport,
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
    ))
}
async fn open_capability(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-capability-openings",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "open-capability",
                None,
                key(&h),
                input.decode::<Value>("mobile-capability-openings")?,
            )
            .await?,
    ))
}
async fn cancel_opening(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-capability-openings",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "cancel-opening",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-capability-openings")?,
            )
            .await?,
    ))
}
async fn request_support(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-adapter-support-requests",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "adapter-support",
                None,
                key(&h),
                input.decode::<Value>("mobile-adapter-support-requests")?,
            )
            .await?,
    ))
}
async fn update_support(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-adapter-support-requests",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "update-adapter-support",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-adapter-support-requests")?,
            )
            .await?,
    ))
}
async fn opening_context(State(a): State<Application>, h: HeaderMap) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-device-opening-contexts",
        a.mobile_execution().opening_context(&c).await?,
    ))
}
async fn report_environment(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-device-environments",
        a.mobile_execution()
            .environment(&c, key(&h), input.decode("mobile-device-environments")?)
            .await?,
    ))
}
async fn capability_context(State(a): State<Application>, h: HeaderMap) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-device-capability-contexts",
        serde_json::json!({"id":c.device_id,"protocol_version":7,"scopes":a.mobile_execution().capability_context(&c).await?}),
    ))
}
async fn capability_decision(
    State(a): State<Application>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-device-capability-consents",
        a.mobile_execution()
            .capability_decision(
                &c,
                key(&h),
                input.decode("mobile-device-capability-consents")?,
            )
            .await?,
    ))
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-target-bindings",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "bind",
                None,
                key(&h),
                input.decode::<Value>("mobile-target-bindings")?,
            )
            .await?,
    ))
}
async fn revoke_binding(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-target-bindings",
        a.mobile_execution()
            .write(
                &actor,
                None,
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
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
async fn platform_bindings(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-target-bindings",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::PlatformBindings,
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
    ))
}
async fn platform_targets(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-targets",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::PlatformTargets,
                f.page(),
                f.search.clone(),
                None,
            )
            .await?,
        &f.page(),
    ))
}
async fn update_binding(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-target-bindings",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "update-binding",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-target-bindings")?,
            )
            .await?,
    ))
}
async fn platform_field_invitations(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-field-check-invitations",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::FieldInvitations,
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
    ))
}
async fn school_field_invitations(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-field-check-invitations",
        a.mobile_execution()
            .list(
                &actor,
                Some(school),
                Resource::FieldInvitations,
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
    ))
}
async fn platform_field_checks(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-field-checks",
        a.mobile_execution()
            .list(
                &actor,
                None,
                Resource::FieldChecks,
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
    ))
}
async fn school_field_checks(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-field-checks",
        a.mobile_execution()
            .list(
                &actor,
                Some(school),
                Resource::FieldChecks,
                f.page(),
                f.search.clone(),
                f.device_id,
            )
            .await?,
        &f.page(),
    ))
}
async fn create_field_invitation(
    State(a): State<Application>,
    actor: Actor,
    Path(school): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-field-check-invitations",
        a.mobile_execution()
            .write(
                &actor,
                Some(school),
                "field-invitation",
                None,
                key(&h),
                input.decode::<Value>("mobile-field-check-invitations")?,
            )
            .await?,
    ))
}
async fn revoke_field_invitation(
    State(a): State<Application>,
    actor: Actor,
    Path((school, id)): Path<(Uuid, Uuid)>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-field-check-invitations",
        a.mobile_execution()
            .write(
                &actor,
                Some(school),
                "revoke-field-invitation",
                Some(id),
                key(&h),
                input.decode::<Value>("mobile-field-check-invitations")?,
            )
            .await?,
    ))
}
async fn request_field_check(
    State(a): State<Application>,
    actor: Actor,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "mobile-field-checks",
        a.mobile_execution()
            .write(
                &actor,
                None,
                "request-field-check",
                None,
                key(&h),
                input.decode::<Value>("mobile-field-checks")?,
            )
            .await?,
    ))
}
async fn device_field_checks(State(a): State<Application>, h: HeaderMap) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-field-check-contexts",
        serde_json::json!({"id":c.device_id,"jobs":a.mobile_execution().field_check_jobs(&c).await?}),
    ))
}
async fn field_check_claim(
    State(a): State<Application>,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-field-check-claims",
        a.mobile_execution()
            .field_check_device_write(
                &c,
                id,
                key(&h),
                "claim",
                input.decode::<Value>("mobile-field-check-claims")?,
            )
            .await?,
    ))
}
async fn field_check_result(
    State(a): State<Application>,
    Path(id): Path<Uuid>,
    h: HeaderMap,
    input: Input,
) -> Result<ApiResponse> {
    let c = device_credential(&a, &h).await?;
    Ok(resource(
        "mobile-field-check-results",
        a.mobile_execution()
            .field_check_device_write(
                &c,
                id,
                key(&h),
                "result",
                input.decode::<Value>("mobile-field-check-results")?,
            )
            .await?,
    ))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v1/schools/{school}/mobile-execution/field-check-invitations",
            get(school_field_invitations).post(create_field_invitation),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/field-check-invitations/{id}/revocations",
            post(revoke_field_invitation),
        )
        .route(
            "/api/v1/schools/{school}/mobile-execution/field-checks",
            get(school_field_checks),
        )
        .route(
            "/api/v1/platform/mobile-execution/field-check-invitations",
            get(platform_field_invitations),
        )
        .route(
            "/api/v1/platform/mobile-execution/field-checks",
            get(platform_field_checks).post(request_field_check),
        )
        .route(
            "/api/v7/device-execution/field-checks",
            get(device_field_checks),
        )
        .route(
            "/api/v7/device-execution/field-checks/{id}/claims",
            post(field_check_claim),
        )
        .route(
            "/api/v7/device-execution/field-checks/{id}/results",
            post(field_check_result),
        )
        .route(
            "/api/v1/platform/mobile-execution/capability-openings",
            get(platform_openings).post(open_capability),
        )
        .route(
            "/api/v1/platform/mobile-execution/capability-openings/{id}/cancellations",
            post(cancel_opening),
        )
        .route(
            "/api/v1/platform/mobile-execution/adapter-releases",
            get(adapter_releases),
        )
        .route(
            "/api/v1/platform/mobile-execution/adapter-support-requests",
            get(adapter_support).post(request_support),
        )
        .route(
            "/api/v7/device-execution/environment-reports",
            post(report_environment),
        )
        .route(
            "/api/v7/device-execution/opening-context",
            get(opening_context),
        )
        .route(
            "/api/v1/platform/mobile-execution/adapter-support-requests/{id}",
            axum::routing::patch(update_support),
        )
        .route(
            "/api/v7/device-execution/capability-context",
            get(capability_context),
        )
        .route(
            "/api/v7/device-execution/capability-consents",
            post(capability_decision),
        )
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
            "/api/v1/platform/mobile-execution/target-bindings",
            get(platform_bindings).post(binding),
        )
        .route(
            "/api/v1/platform/mobile-execution/target-bindings/{id}",
            axum::routing::patch(update_binding),
        )
        .route(
            "/api/v1/platform/mobile-execution/target-bindings/{id}/revocations",
            post(revoke_binding),
        )
        .route(
            "/api/v1/platform/mobile-execution/targets",
            get(platform_targets),
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
            "/api/v1/schools/{school}/mobile-execution/target-bindings",
            get(bindings),
        )
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use axum::http::Uri;

    #[test]
    fn pagination_and_empty_search_parse_from_client_url() {
        for resource in [
            "target-bindings",
            "capability-openings",
            "devices",
            "targets",
        ] {
            let uri: Uri = format!(
                "/api/v1/platform/mobile-execution/{resource}?page[number]=1&page[size]=30&search="
            )
            .parse()
            .unwrap();
            let Query(filter) = Query::<Filter>::try_from_uri(&uri).unwrap();
            assert_eq!(filter.page().bounds().unwrap(), (30, 0), "{resource}");
            assert!(filter.search.is_empty(), "{resource}");
            assert!(filter.device_id.is_none(), "{resource}");
        }
    }

    #[test]
    fn encoded_pagination_search_and_device_filter_parse() {
        let device = Uuid::from_u128(1);
        let uri: Uri = format!(
            "/api/query?page%5Bnumber%5D=2&page%5Bsize%5D=10&search=%E7%BE%A4&device_id={device}"
        )
        .parse()
        .unwrap();
        let Query(filter) = Query::<Filter>::try_from_uri(&uri).unwrap();
        assert_eq!(filter.page().bounds().unwrap(), (10, 10));
        assert_eq!(filter.search, "群");
        assert_eq!(filter.device_id, Some(device));
    }

    #[test]
    fn query_defaults_and_validation_are_preserved() {
        let Query(filter) = Query::<Filter>::try_from_uri(&"/api/query".parse().unwrap()).unwrap();
        assert_eq!(filter.page().bounds().unwrap(), (30, 0));
        for query in [
            "page[number]=abc",
            "device_id=invalid",
            "page_size=10",
            "page=1",
        ] {
            let uri = format!("/api/query?{query}").parse().unwrap();
            assert!(Query::<Filter>::try_from_uri(&uri).is_err(), "{query}");
        }
        for query in ["page[number]=0", "page[size]=101", "page[size]=-1"] {
            let uri = format!("/api/query?{query}").parse().unwrap();
            let Query(filter) = Query::<Filter>::try_from_uri(&uri).unwrap();
            assert!(filter.page().bounds().is_err(), "{query}");
        }
    }
}
