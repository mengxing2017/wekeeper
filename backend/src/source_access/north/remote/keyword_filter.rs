use super::*;
use crate::source_access::{
    domain::keyword_filter::{Command, Publication},
    north::local::keyword_filter::Service,
    south::adapter::postgres::keyword_filter::KeywordRepository,
};
#[derive(Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Page {
    page: Option<i64>,
}
async fn execute(
    a: Application,
    actor: Actor,
    command: Command,
    kind: &str,
) -> std::result::Result<Response, Response> {
    let value = Service(KeywordRepository(a.pool))
        .execute(&actor, command)
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton(kind, "result", value).into_response())
}
async fn scopes(
    State(a): State<Application>,
    actor: Actor,
) -> std::result::Result<Response, Response> {
    execute(a, actor, Command::Scopes, "keyword-rule-scopes").await
}
async fn versions(
    State(a): State<Application>,
    actor: Actor,
    Path(scope): Path<String>,
    Query(page): Query<Page>,
) -> std::result::Result<Response, Response> {
    execute(
        a,
        actor,
        Command::Versions {
            scope,
            page: page.page.unwrap_or(1),
        },
        "keyword-rule-versions",
    )
    .await
}
async fn publish(
    State(a): State<Application>,
    actor: Actor,
    Path(scope): Path<String>,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let input = input
        .decode::<Publication>("keyword-rule-publications")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    execute(
        a,
        actor,
        Command::Publish { scope, input },
        "keyword-rule-publications",
    )
    .await
}
async fn checks(
    State(a): State<Application>,
    actor: Actor,
    Path((route, receipt)): Path<(String, Uuid)>,
    Query(page): Query<Page>,
) -> std::result::Result<Response, Response> {
    execute(
        a,
        actor,
        Command::Checks {
            route,
            receipt,
            page: page.page.unwrap_or(1),
        },
        "keyword-check-results",
    )
    .await
}
async fn hits(
    State(a): State<Application>,
    actor: Actor,
    Path((route, receipt, check)): Path<(String, Uuid, Uuid)>,
    Query(page): Query<Page>,
) -> std::result::Result<Response, Response> {
    execute(
        a,
        actor,
        Command::Hits {
            route,
            receipt,
            check,
            page: page.page.unwrap_or(1),
        },
        "keyword-check-hits",
    )
    .await
}
use uuid::Uuid;
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v2/admin/keyword-rule-scopes", get(scopes))
        .route(
            "/api/v2/admin/keyword-rule-scopes/{scope}/versions",
            get(versions),
        )
        .route(
            "/api/v2/admin/keyword-rule-scopes/{scope}/publications",
            post(publish),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/receipts/{receipt}/checks",
            get(checks),
        )
        .route(
            "/api/v2/admin/source-routes/{route}/receipts/{receipt}/checks/{check}/hits",
            get(hits),
        )
}
