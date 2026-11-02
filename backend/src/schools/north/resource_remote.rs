use crate::{
    bootstrap::Application,
    business::{Actor, Result},
    distribution::domain::workflows::Confirmation,
    http::jsonapi::{ApiResponse, Input, resource},
    schools::{
        north::resources::Service,
        south::{adapter::postgres::Repository, port::resources::Command},
    },
};
use axum::{
    Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::Deserialize;
use uuid::Uuid;
fn service(a: &Application) -> Service {
    Service(Repository(a.pool.clone()))
}
fn binary((mime, b): (String, Vec<u8>)) -> Response {
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "private, no-store".into()),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".into()),
        ],
        b,
    )
        .into_response()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QrInput {
    url: String,
}
macro_rules! scope_handlers {
    ($pre:ident,$con:ident,$up:ident,$list:ident,$get:ident,$qr:ident,$scope:expr,$path:ty,$binding:pat) => {
        async fn $pre(
            State(a): State<Application>,
            actor: Actor,
            Path($binding): Path<$path>,
            input: Input,
        ) -> Result<ApiResponse> {
            Ok(resource(
                "resource-previews",
                service(&a)
                    .command(
                        &actor,
                        $scope,
                        Command::Preview(input.decode("resource-previews")?),
                    )
                    .await?,
            ))
        }
        async fn $con(
            State(a): State<Application>,
            actor: Actor,
            Path($binding): Path<$path>,
            input: Input,
        ) -> Result<ApiResponse> {
            let c: Confirmation = input.decode("resource-publications")?;
            Ok(resource(
                "resources",
                service(&a)
                    .command(
                        &actor,
                        $scope,
                        Command::Confirm {
                            preview_id: c.preview_id,
                            digest: c.digest,
                        },
                    )
                    .await?,
            ))
        }
        async fn $up(
            State(a): State<Application>,
            actor: Actor,
            Path($binding): Path<$path>,
            bytes: Bytes,
        ) -> Result<ApiResponse> {
            Ok(resource(
                "resource-artwork",
                service(&a).upload(&actor, $scope, bytes.to_vec()).await?,
            ))
        }
        async fn $list(
            State(a): State<Application>,
            actor: Actor,
            Path($binding): Path<$path>,
        ) -> Result<ApiResponse> {
            Ok(resource(
                "resource-artwork",
                service(&a).list(&actor, $scope).await?,
            ))
        }
        async fn $qr(
            State(a): State<Application>,
            actor: Actor,
            Path($binding): Path<$path>,
            input: Input,
        ) -> Result<ApiResponse> {
            let v: QrInput = input.decode("resource-qrcodes")?;
            Ok(resource(
                "resource-artwork",
                service(&a).qr(&actor, $scope, &v.url).await?,
            ))
        }
    };
}
scope_handlers!(preview, confirm, upload, list, unused, qr, Some(s), Uuid, s);
// Platform routes do not have a school path parameter.
async fn platform_preview(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "resource-previews",
        service(&a)
            .command(
                &actor,
                None,
                Command::Preview(input.decode("resource-previews")?),
            )
            .await?,
    ))
}
async fn platform_confirm(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    let c: Confirmation = input.decode("resource-publications")?;
    Ok(resource(
        "resources",
        service(&a)
            .command(
                &actor,
                None,
                Command::Confirm {
                    preview_id: c.preview_id,
                    digest: c.digest,
                },
            )
            .await?,
    ))
}
async fn platform_upload(
    State(a): State<Application>,
    actor: Actor,
    bytes: Bytes,
) -> Result<ApiResponse> {
    Ok(resource(
        "resource-artwork",
        service(&a).upload(&actor, None, bytes.to_vec()).await?,
    ))
}
async fn platform_list(State(a): State<Application>, actor: Actor) -> Result<ApiResponse> {
    Ok(resource(
        "resource-artwork",
        service(&a).list(&actor, None).await?,
    ))
}
async fn platform_qr(
    State(a): State<Application>,
    actor: Actor,
    input: Input,
) -> Result<ApiResponse> {
    let v: QrInput = input.decode("resource-qrcodes")?;
    Ok(resource(
        "resource-artwork",
        service(&a).qr(&actor, None, &v.url).await?,
    ))
}
async fn artwork(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<Response> {
    Ok(binary(service(&a).media(&actor, Some(s), id, false).await?))
}
async fn platform_artwork(
    State(a): State<Application>,
    actor: Actor,
    Path(id): Path<Uuid>,
) -> Result<Response> {
    Ok(binary(service(&a).media(&actor, None, id, false).await?))
}
async fn published(
    State(a): State<Application>,
    actor: Actor,
    Path((s, id)): Path<(Uuid, Uuid)>,
) -> Result<Response> {
    Ok(binary(service(&a).media(&actor, Some(s), id, true).await?))
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/schools/{school}/resource-previews", post(preview))
        .route(
            "/api/v1/schools/{school}/resource-publications",
            post(confirm),
        )
        .route("/api/v1/platform/resource-previews", post(platform_preview))
        .route(
            "/api/v1/platform/resource-publications",
            post(platform_confirm),
        )
        .route(
            "/api/v1/schools/{school}/resource-artwork",
            get(list).post(upload),
        )
        .route(
            "/api/v1/schools/{school}/resource-artwork/{id}",
            get(artwork),
        )
        .route("/api/v1/schools/{school}/resource-qrcodes", post(qr))
        .route(
            "/api/v1/platform/resource-artwork",
            get(platform_list).post(platform_upload),
        )
        .route(
            "/api/v1/platform/resource-artwork/{id}",
            get(platform_artwork),
        )
        .route("/api/v1/platform/resource-qrcodes", post(platform_qr))
        .route(
            "/api/v1/schools/{school}/resource-media/{id}",
            get(published),
        )
        .layer(DefaultBodyLimit::max(5 * 1024 * 1024))
}
