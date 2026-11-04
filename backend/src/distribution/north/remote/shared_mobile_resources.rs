use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    distribution::south::port::shared_mobile_resources::Resource,
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
async fn platform_devices(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-devices",
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources().target(&actor, s, id).await?,
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
async fn platform_bindings(
    State(a): State<Application>,
    actor: Actor,
    Query(f): Query<Filter>,
) -> Result<ApiResponse> {
    Ok(collection(
        "mobile-target-bindings",
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
        a.shared_mobile_resources()
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
pub fn router() -> Router<Application> {
    Router::new()
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
        for resource in ["target-bindings", "devices", "targets"] {
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
