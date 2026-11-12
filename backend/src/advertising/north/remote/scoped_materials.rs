use crate::{
    advertising::{
        domain::{
            AdvertisingError,
            targeting::{
                ArchiveMaterial, CreateMaterial, CreatePlacementRule, PublishMaterial,
                SetPlacementState,
            },
        },
        north::local::{MaterialService, PlacementRuleService},
        south::adapter::postgres::Repository,
    },
    bootstrap::Application,
    business::Actor,
    http::jsonapi::Input,
};
use axum::{
    Router,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use uuid::Uuid;

fn failure(error: AdvertisingError) -> Response {
    let (status, detail) = match error.0 {
        "forbidden" => (StatusCode::FORBIDDEN, "没有广告管理权限"),
        "not_found" => (StatusCode::NOT_FOUND, "资源不存在"),
        "revision_conflict" => (StatusCode::CONFLICT, "记录已更新，请刷新后重试"),
        "material_archived" => (StatusCode::CONFLICT, "素材已归档"),
        "material_unpublished" => (StatusCode::CONFLICT, "素材尚未发布"),
        "invalid_request" | "invalid_ad_content" | "invalid_ad_link" | "invalid_ad_scope" => {
            (StatusCode::BAD_REQUEST, "广告内容或请求不合法")
        }
        _ => (StatusCode::SERVICE_UNAVAILABLE, "广告服务暂不可用"),
    };
    crate::http::error::response(status, error.0, detail)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    page: Option<i64>,
}

async fn list_materials(
    State(app): State<Application>,
    actor: Actor,
    Query(query): Query<ListQuery>,
) -> Result<Response, Response> {
    let result = MaterialService(Repository(app.pool))
        .list(&actor, query.page.unwrap_or(1))
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::singleton("ad-materials", "list", result).into_response())
}
async fn list_rules(
    State(app): State<Application>,
    actor: Actor,
    Query(query): Query<ListQuery>,
) -> Result<Response, Response> {
    let result = PlacementRuleService(Repository(app.pool))
        .list(&actor, query.page.unwrap_or(1))
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::singleton("ad-placement-rules", "list", result).into_response())
}

async fn create(
    State(app): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<Response, Response> {
    let input = input
        .decode::<CreateMaterial>("ad-materials")
        .map_err(|_| failure(AdvertisingError("invalid_request")))?;
    let result = MaterialService(Repository(app.pool))
        .create(&actor, input)
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::resource("ad-materials", result).into_response())
}
async fn get_material(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<Response, Response> {
    let result = MaterialService(Repository(app.pool))
        .get(&actor, id)
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::resource("ad-materials", result).into_response())
}
async fn archive(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<Response, Response> {
    let input = input
        .decode::<ArchiveMaterial>("ad-material-archives")
        .map_err(|_| failure(AdvertisingError("invalid_request")))?;
    let result = MaterialService(Repository(app.pool))
        .archive(&actor, id, input)
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::resource("ad-materials", result).into_response())
}
async fn versions(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    Query(query): Query<ListQuery>,
) -> Result<Response, Response> {
    let result = MaterialService(Repository(app.pool))
        .versions(&actor, id, query.page.unwrap_or(1))
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::singleton("ad-material-versions", "list", result).into_response())
}
async fn publish(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<Response, Response> {
    let input = input
        .decode::<PublishMaterial>("ad-material-publications")
        .map_err(|_| failure(AdvertisingError("invalid_request")))?;
    let result = MaterialService(Repository(app.pool))
        .publish(&actor, id, input)
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::resource("ad-materials", result).into_response())
}
async fn create_rule(
    State(app): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<Response, Response> {
    let input = input
        .decode::<CreatePlacementRule>("ad-placement-rules")
        .map_err(|_| failure(AdvertisingError("invalid_request")))?;
    let result = PlacementRuleService(Repository(app.pool))
        .create(&actor, input)
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::resource("ad-placement-rules", result).into_response())
}
async fn get_rule(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<Response, Response> {
    let result = PlacementRuleService(Repository(app.pool))
        .get(&actor, id)
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::resource("ad-placement-rules", result).into_response())
}
async fn set_rule_state(
    State(app): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
    input: Input,
) -> Result<Response, Response> {
    let input = input
        .decode::<SetPlacementState>("ad-placement-rule-states")
        .map_err(|_| failure(AdvertisingError("invalid_request")))?;
    let result = PlacementRuleService(Repository(app.pool))
        .set_state(&actor, id, input)
        .await
        .map_err(failure)?;
    Ok(crate::http::jsonapi::resource("ad-placement-rules", result).into_response())
}
pub fn router() -> Router<Application> {
    Router::new()
        .route(
            "/api/v2/admin/ad-materials",
            get(list_materials).post(create),
        )
        .route("/api/v2/admin/ad-materials/{id}", get(get_material))
        .route("/api/v2/admin/ad-materials/{id}/versions", get(versions))
        .route("/api/v2/admin/ad-materials/{id}/archive", put(archive))
        .route(
            "/api/v2/admin/ad-materials/{id}/publications",
            post(publish),
        )
        .route(
            "/api/v2/admin/ad-placement-rules",
            get(list_rules).post(create_rule),
        )
        .route("/api/v2/admin/ad-placement-rules/{id}", get(get_rule))
        .route(
            "/api/v2/admin/ad-placement-rules/{id}/state",
            put(set_rule_state),
        )
}
