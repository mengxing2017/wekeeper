use crate::{
    bootstrap::Application,
    business::{Actor, Error, Result, require, text},
    http::error::data,
    identity::north::remote::{check_origin, cookie},
};
use axum::{
    Json, Router,
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
async fn start(State(a): State<Application>) -> Result<Response> {
    if !a.integrations.configured() {
        return Err(Error::Unavailable);
    }
    let state = a.integrations.repository.oauth_state().await?;
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
    State(a): State<Application>,
    h: HeaderMap,
    Query(v): Query<Callback>,
) -> Result<Response> {
    let state = h
        .get(header::COOKIE)
        .and_then(|x| x.to_str().ok())
        .and_then(|s| {
            s.split(';')
                .find_map(|x| x.trim().strip_prefix("wekeeper_oauth="))
        })
        .ok_or(Error::Forbidden)?;
    require(state == v.state, "OAuth state 不匹配")?;
    a.integrations.repository.consume(&v.state).await?;
    let id = a.integrations.oauth_member(&v.code).await?;
    let (token, _) = a.identity.0.session(id, "browser", true).await?;
    Ok((
        [
            (
                header::SET_COOKIE,
                cookie(&token, !a.options.insecure_local_auth, 28800),
            ),
            (header::CACHE_CONTROL, "no-store".into()),
        ],
        Redirect::to("/wecom"),
    )
        .into_response())
}
#[derive(Deserialize)]
struct Sign {
    url: String,
    #[serde(default)]
    agent: bool,
}
async fn sign(
    State(a): State<Application>,
    actor: Actor,
    Query(v): Query<Sign>,
) -> Result<Response> {
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        data(
            a.integrations
                .signature(&actor, &v.url, &a.options.public_origin, v.agent)
                .await?,
        ),
    )
        .into_response())
}
async fn info(State(a): State<Application>, _actor: Actor) -> Json<Value> {
    data(json!({"corpId":a.integrations.corp,"agentId":a.integrations.agent}))
}
async fn mini(
    State(a): State<Application>,
    h: HeaderMap,
    Json(v): Json<Value>,
) -> Result<Response> {
    if h.contains_key(header::ORIGIN) {
        check_origin(&h, &a)?;
    }
    let code = text(v["code"].as_str().unwrap_or(""), 1, 256)?;
    let id = a.integrations.miniapp(&code).await?;
    let (token, user) = a.identity.0.session(id, "miniapp", false).await?;
    Ok((
        [(header::CACHE_CONTROL, "no-store")],
        data(json!({"token":token,"user":user})),
    )
        .into_response())
}
pub fn router() -> Router<Application> {
    Router::new()
        .route("/api/v1/wecom/oauth/start", get(start))
        .route("/api/v1/wecom/oauth/callback", get(callback))
        .route("/api/v1/wecom/js-sdk-config", get(sign))
        .route("/api/v1/wecom/config", get(info))
        .route("/api/v1/auth/miniapp", post(mini))
}
