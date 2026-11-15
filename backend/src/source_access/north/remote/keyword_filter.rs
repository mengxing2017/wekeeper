use super::*;
use crate::source_access::{
    domain::keyword_filter::{Command, Publication},
    north::local::keyword_filter::Service,
    south::adapter::postgres::keyword_filter::KeywordRepository,
};
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
#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct KeywordUpdate {
    expected_revision: i64,
    entries: Vec<crate::source_access::domain::keyword_filter::Entry>,
}
async fn words(
    State(a): State<Application>,
    actor: Actor,
) -> std::result::Result<Response, Response> {
    let value = Service(KeywordRepository(a.pool))
        .execute(
            &actor,
            Command::Versions {
                scope: "global".into(),
                page: 1,
            },
        )
        .await
        .map_err(|e| e.response(&actor.request_id))?;
    let current = value["items"]
        .as_array()
        .and_then(|items| items.first())
        .ok_or_else(|| Fault("keyword_rules_unavailable").response(&actor.request_id))?;
    Ok(crate::http::jsonapi::singleton(
        "sensitive-words",
        "current",
        serde_json::json!({"revision":current["revision"],"entries":current["entries"]}),
    )
    .into_response())
}
async fn update_words(
    State(a): State<Application>,
    actor: Actor,
    input: crate::http::jsonapi::Input,
) -> std::result::Result<Response, Response> {
    let input = input
        .decode::<KeywordUpdate>("sensitive-words")
        .map_err(|_| Fault("invalid_request").response(&actor.request_id))?;
    execute(
        a,
        actor,
        Command::Publish {
            scope: "global".into(),
            input: Publication {
                expected_revision: input.expected_revision,
                reason: "维护敏感词".into(),
                entries: input.entries,
            },
        },
        "sensitive-words",
    )
    .await
}
pub fn router() -> Router<Application> {
    Router::new().route(
        "/api/v2/admin/sensitive-words",
        get(words).put(update_words),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sensitive_words_update_matches_public_contract() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../../api-contract/fixtures/source-keywords.json"
        ))
        .unwrap();
        let update: KeywordUpdate = serde_json::from_value(fixture["update"].clone()).unwrap();
        assert_eq!(serde_json::to_value(update).unwrap(), fixture["update"]);
        let mut invalid = fixture["update"].clone();
        invalid["reason"] = serde_json::json!("unexpected");
        assert!(serde_json::from_value::<KeywordUpdate>(invalid).is_err());
    }
}
