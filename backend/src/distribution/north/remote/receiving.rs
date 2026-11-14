use crate::{
    bootstrap::Application,
    business::{Actor, Page, Result},
    distribution::domain::receiving::{Channel, ConfigurationInput, TargetInput},
    http::jsonapi::{ApiResponse, Input, resource, singleton},
};
use axum::{
    Router,
    extract::{Path, Query, State},
    routing::{get, put},
};
use uuid::Uuid;

// The channel is fixed by the registered router, never inferred from user JSON.
async fn list(
    State(app): State<Application>,
    actor: Actor,
    axum::Extension(channel): axum::Extension<Channel>,
    Path(unit): Path<Uuid>,
    Query(page): Query<Page>,
) -> Result<ApiResponse> {
    let mut items = app
        .receiving_configurations()
        .list(&actor, channel, unit, page.clone())
        .await?;
    let size = page.page_size.unwrap_or(30) as usize;
    let more = items.len() > size;
    items.truncate(size);
    Ok(resource(channel.kind(), items).meta(
        serde_json::json!({"page":{"number":page.page.unwrap_or(1),"size":size,"has_more":more}}),
    ))
}

async fn resources(
    State(app): State<Application>,
    actor: Actor,
    axum::Extension(channel): axum::Extension<Channel>,
    Path(unit): Path<Uuid>,
) -> Result<ApiResponse> {
    Ok(singleton(
        "receiving-resources",
        unit,
        app.receiving_configurations()
            .resources(&actor, channel, unit)
            .await?,
    ))
}

async fn target(
    State(app): State<Application>,
    actor: Actor,
    axum::Extension(channel): axum::Extension<Channel>,
    Path((unit, id)): Path<(Uuid, Uuid)>,
    input: Input,
) -> Result<ApiResponse> {
    Ok(resource(
        "receiving-targets",
        app.receiving_configurations()
            .target(
                &actor,
                channel,
                unit,
                id,
                input.decode::<TargetInput>("receiving-targets")?,
            )
            .await?,
    ))
}

async fn detail(
    State(app): State<Application>,
    actor: Actor,
    axum::Extension(channel): axum::Extension<Channel>,
    Path((unit, id)): Path<(Uuid, Uuid)>,
) -> Result<ApiResponse> {
    Ok(resource(
        channel.kind(),
        app.receiving_configurations()
            .get(&actor, channel, unit, id)
            .await?,
    ))
}

async fn save(
    State(app): State<Application>,
    actor: Actor,
    axum::Extension(channel): axum::Extension<Channel>,
    Path((unit, id)): Path<(Uuid, Uuid)>,
    input: Input,
) -> Result<ApiResponse> {
    let input: ConfigurationInput = input.decode(channel.kind())?;
    Ok(resource(
        channel.kind(),
        app.receiving_configurations()
            .save(&actor, channel, unit, id, input)
            .await?,
    ))
}

pub fn router() -> Router<Application> {
    let mut router = Router::new();
    for (base, channel) in [
        ("mobile-execution", Channel::Mobile),
        ("wecom-distribution", Channel::Wecom),
    ] {
        router = router.merge(
            Router::new()
                .route(
                    &format!("/api/v1/{base}/business-units/{{unit}}/receiving-targets/{{id}}"),
                    put(target),
                )
                .route(
                    &format!("/api/v1/{base}/business-units/{{unit}}/receiving-resources"),
                    get(resources),
                )
                .route(
                    &format!("/api/v1/{base}/business-units/{{unit}}/receiving-configurations"),
                    get(list),
                )
                .route(
                    &format!(
                        "/api/v1/{base}/business-units/{{unit}}/receiving-configurations/{{id}}"
                    ),
                    get(detail).put(save),
                )
                .layer(axum::Extension(channel)),
        );
    }
    router
}
