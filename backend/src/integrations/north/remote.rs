use crate::{
    bootstrap::Application,
    business::{Actor, Error, Result, require, text},
    http::jsonapi::{Input, singleton},
    identity::north::remote::{check_origin, cookie},
};
use axum::{
    Router,
    extract::{Query, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
};
use serde::Deserialize;
use serde_json::{Value, json};
#[derive(Deserialize)]
struct Callback {
    code: String,
    state: String,
}
#[derive(Deserialize, Default)]
struct Start {
    school: Option<uuid::Uuid>,
    draft: Option<uuid::Uuid>,
}
impl Start {
    fn return_path(&self) -> String {
        let mut query = vec![];
        if let Some(id) = self.school {
            query.push(format!("school={id}"));
        }
        if let Some(id) = self.draft {
            query.push(format!("draft={id}"));
        }
        if query.is_empty() {
            "/wecom".into()
        } else {
            format!("/wecom?{}", query.join("&"))
        }
    }
}
async fn start(State(a): State<Application>, Query(input): Query<Start>) -> Result<Response> {
    if !oauth_available(&a) {
        return Err(Error::Unavailable);
    }
    let state = a
        .integrations
        .repository
        .oauth_state(&input.return_path())
        .await?;
    let mut url = reqwest::Url::parse("https://open.weixin.qq.com/connect/oauth2/authorize")
        .expect("constant URL");
    url.query_pairs_mut()
        .append_pair("appid", &a.integrations.corp)
        .append_pair(
            "redirect_uri",
            &format!("{}/api/v1/wecom/oauth/callback", a.options.public_origin),
        )
        .append_pair("response_type", "code")
        .append_pair("scope", "snsapi_base")
        .append_pair("state", &state);
    url.set_fragment(Some("wechat_redirect"));
    Ok(([(header::SET_COOKIE,format!("wekeeper_oauth={state}; Path=/api/v1/wecom/oauth; HttpOnly; SameSite=Lax; Max-Age=300{}",if a.options.insecure_local_auth{""}else{"; Secure"})),(header::CACHE_CONTROL,"no-store".into())],Redirect::to(url.as_str())).into_response())
}
async fn callback(
    state: State<Application>,
    headers: HeaderMap,
    query: Query<Callback>,
) -> Response {
    match callback_inner(state, headers, query).await {
        Ok(response) => response,
        Err(_) => (
            [(header::CACHE_CONTROL, "no-store")],
            Redirect::to("/wecom?oauth_error=failed"),
        )
            .into_response(),
    }
}
async fn callback_inner(
    State(a): State<Application>,
    h: HeaderMap,
    Query(v): Query<Callback>,
) -> Result<Response> {
    require(oauth_available(&a), "当前环境不开放企业微信登录")?;
    let state = h
        .get(header::COOKIE)
        .and_then(|x| x.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find_map(|x| x.trim().strip_prefix("wekeeper_oauth="))
        })
        .ok_or(Error::Forbidden)?;
    require(state == v.state, "OAuth state 不匹配")?;
    let return_path = a.integrations.repository.consume(&v.state).await?;
    let (id, userid, generation) = a.integrations.oauth_member(&v.code).await?;
    let (token, actor) = a.identity.0.session(id, "browser", true).await?;
    if !a
        .integrations
        .repository
        .session_member_matches(id, actor.wecom_binding_revision, &userid, generation)
        .await?
    {
        a.identity.0.logout(&actor.session_hash).await?;
        return Err(Error::Conflict);
    }
    Ok((
        axum::response::AppendHeaders([
            (
                header::SET_COOKIE,
                cookie(&token, !a.options.insecure_local_auth, 28800),
            ),
            (header::SET_COOKIE, format!("wekeeper_oauth=; Path=/api/v1/wecom/oauth; HttpOnly; SameSite=Lax; Max-Age=0{}", if a.options.insecure_local_auth { "" } else { "; Secure" })),
            (header::CACHE_CONTROL, "no-store".into()),
        ]),
        Redirect::to(&return_path),
    )
        .into_response())
}
async fn retired_share(_actor: Actor) -> Result<Response> {
    Err(Error::Invalid("逐群分享已停用，请创建企业群发任务".into()))
}
fn oauth_available(a: &Application) -> bool {
    a.integrations
        .capabilities(&a.options.public_origin, a.options.insecure_local_auth)
        .oauth_available
}
async fn status(State(a): State<Application>) -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        singleton(
            "wecom-status",
            "current",
            a.integrations
                .capabilities(&a.options.public_origin, a.options.insecure_local_auth),
        ),
    )
        .into_response()
}
async fn mini(
    State(a): State<Application>,
    h: HeaderMap,
    input_document: Input,
) -> Result<Response> {
    let v = input_document.decode::<Value>("sessions")?;
    if h.contains_key(header::ORIGIN) {
        check_origin(&h, &a)?;
    }
    let code = text(v["code"].as_str().unwrap_or(""), 1, 256)?;
    let id = a.integrations.miniapp(&code).await?;
    let (token, user) = a.identity.0.session(id, "miniapp", false).await?;
    Ok(miniapp_session_response(token, user))
}
fn miniapp_session_response(token: String, user: Actor) -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        singleton("sessions", "current", json!({"token":token,"user":user})),
    )
        .into_response()
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/wecom/status", get(status))
        .route("/api/v1/wecom/oauth/start", get(start))
        .route("/api/v1/wecom/oauth/callback", get(callback))
        .route("/api/v1/wecom/js-sdk-config", get(retired_share))
        .route("/api/v1/wecom/config", get(retired_share))
        .route("/api/v1/auth/miniapp", post(mini))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn miniapp_session_matches_client_fixture_without_browser_secrets() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../../../api-contract/fixtures/miniapp-session.json"
        ))
        .unwrap();
        let mut value = fixture["data"]["attributes"]["user"].clone();
        // Actor's private browser fields must never escape into the response.
        value["csrf"] = json!("must-not-be-serialized");
        let user: Actor = serde_json::from_value(value).unwrap();
        let response = miniapp_session_response("fixture-only-miniapp-token".into(), user);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        let body = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let actual: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(actual, fixture);
    }
    #[test]
    fn oauth_return_path_is_local_and_preserves_selection() {
        assert_eq!(Start::default().return_path(), "/wecom");
        let id = uuid::Uuid::nil();
        assert_eq!(
            Start {
                school: Some(id),
                draft: Some(id)
            }
            .return_path(),
            format!("/wecom?school={id}&draft={id}")
        );
    }
}
