use super::super::south::adapter::postgres::Repository;
use crate::business::{Actor, Error, Result, require, secret};
use crate::integrations::domain::{group_contains_member, sdk_signature};
use serde_json::{Value, json};
#[derive(Clone)]
pub struct Service {
    pub repository: Repository,
    pub(crate) client: reqwest::Client,
    pub corp: String,
    pub agent: String,
    secret: String,
    #[cfg(test)]
    api_origin: Option<String>,
    wx_app: String,
    wx_secret: String,
}
impl Service {
    pub fn new(pool: sqlx::PgPool) -> Self {
        let env = |k| std::env::var(k).unwrap_or_default();
        Self {
            repository: Repository(pool),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("TLS client"),
            corp: env("WEKEEPER_WECOM_CORP_ID"),
            agent: env("WEKEEPER_WECOM_AGENT_ID"),
            secret: env("WEKEEPER_WECOM_SECRET"),
            #[cfg(test)]
            api_origin: None,
            wx_app: env("WEKEEPER_WX_APP_ID"),
            wx_secret: env("WEKEEPER_WX_SECRET"),
        }
    }
    pub fn server_configured(&self) -> bool {
        !self.corp.trim().is_empty() && !self.secret.trim().is_empty()
    }
    pub fn client_configured(&self) -> bool {
        self.server_configured()
            && self.agent.bytes().all(|b| b.is_ascii_digit())
            && self.agent.parse::<u32>().is_ok_and(|id| id > 0)
    }
    pub fn capabilities(&self, origin: &str, insecure: bool) -> WecomStatus {
        let deployment = !insecure
            && reqwest::Url::parse(origin)
                .is_ok_and(|url| url.scheme() == "https" && url.host_str().is_some());
        WecomStatus {
            server_configured: self.server_configured(),
            client_configured: self.client_configured(),
            oauth_available: self.server_configured() && deployment,
            share_available: self.client_configured() && deployment,
        }
    }
    pub(crate) fn credential_scope(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut digest = Sha256::new();
        digest.update(b"wekeeper:wecom:credentials:v2");
        for value in [&self.corp, &self.secret] {
            digest.update((value.len() as u64).to_be_bytes());
            digest.update(value.as_bytes());
        }
        format!("{:x}", digest.finalize())
    }
    pub(crate) fn cache_key(&self, kind: &str) -> String {
        format!("wecom:v2:{}:{kind}", self.credential_scope())
    }
    pub(crate) fn endpoint(&self, path: &str) -> String {
        #[cfg(test)]
        if let Some(origin) = &self.api_origin {
            return format!("{origin}/cgi-bin/{path}");
        }
        format!("https://qyapi.weixin.qq.com/cgi-bin/{path}")
    }
    async fn json(&self, request: reqwest::RequestBuilder) -> Result<Value> {
        let response = request.send().await.map_err(|_| Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        let v: Value = response.json().await.map_err(|_| Error::Unavailable)?;
        if v["errcode"].as_i64().unwrap_or(0) != 0 {
            // Never log the URL, token, secret, OAuth code or upstream response body.
            tracing::warn!(
                errcode = v["errcode"].as_i64(),
                "official platform API rejected request"
            );
            return Err(Error::Unavailable);
        }
        Ok(v)
    }
    pub(crate) async fn token(&self) -> Result<String> {
        if !self.server_configured() {
            return Err(Error::Unavailable);
        }
        let key = self.cache_key("token");
        self.repository
            .cached_or_fetch(&key, || async {
                let v = self
                    .json(
                        self.client
                            .get(self.endpoint("gettoken"))
                            .query(&[("corpid", &self.corp), ("corpsecret", &self.secret)]),
                    )
                    .await?;
                Ok((
                    v["access_token"]
                        .as_str()
                        .ok_or(Error::Unavailable)?
                        .to_owned(),
                    v["expires_in"].as_i64().unwrap_or(7200) - 300,
                ))
            })
            .await
    }
    pub async fn oauth_member(&self, code: &str) -> Result<(uuid::Uuid, String, i64)> {
        let token = self.token().await?;
        let v = self
            .json(
                self.client
                    .get(self.endpoint("auth/getuserinfo"))
                    .query(&[("access_token", token.as_str()), ("code", code)]),
            )
            .await?;
        let userid = v["userid"]
            .as_str()
            .or(v["UserId"].as_str())
            .ok_or(Error::Forbidden)?;
        let generation = self.directory().verify(userid).await?;
        let id = self
            .repository
            .member(
                v["userid"]
                    .as_str()
                    .or(v["UserId"].as_str())
                    .ok_or(Error::Forbidden)?,
            )
            .await?;
        Ok((id, userid.to_owned(), generation))
    }
    pub async fn miniapp(&self, code: &str) -> Result<uuid::Uuid> {
        require(
            !self.wx_app.is_empty() && !self.wx_secret.is_empty(),
            "尚未配置微信小程序",
        )?;
        let v = self
            .json(
                self.client
                    .get("https://api.weixin.qq.com/sns/jscode2session")
                    .query(&[
                        ("appid", self.wx_app.as_str()),
                        ("secret", self.wx_secret.as_str()),
                        ("js_code", code),
                        ("grant_type", "authorization_code"),
                    ]),
            )
            .await?;
        self.repository
            .wx_account(v["openid"].as_str().ok_or(Error::Forbidden)?)
            .await
    }
    pub async fn validate_group(&self, corp: &str, chat: &str) -> Result<()> {
        require(
            !self.corp.is_empty() && corp == self.corp,
            "企业 ID 必须与当前应用配置一致",
        )?;
        let token = self.token().await?;
        let v = self
            .json(
                self.client
                    .post("https://qyapi.weixin.qq.com/cgi-bin/externalcontact/groupchat/get")
                    .query(&[("access_token", token)])
                    .json(&json!({"chat_id":chat,"need_name":0})),
            )
            .await?;
        require(
            v["group_chat"]["chat_id"].as_str() == Some(chat),
            "无法核验目标客户群",
        )
    }
    pub async fn verify_group(&self, a: &Actor, corp: &str, chat: &str) -> Result<()> {
        require(a.wecom_authenticated && corp == self.corp, "企业身份不匹配")?;
        self.directory().verify_account(a.id).await?;
        let userid = self.repository.userid(a.id).await?;
        let token = self.token().await?;
        let v = self
            .json(
                self.client
                    .post("https://qyapi.weixin.qq.com/cgi-bin/externalcontact/groupchat/get")
                    .query(&[("access_token", token)])
                    .json(&json!({"chat_id":chat,"need_name":0})),
            )
            .await?;
        let g = &v["group_chat"];
        require(g["chat_id"].as_str() == Some(chat), "群信息不匹配")?;
        require(
            group_contains_member(g, chat, &userid),
            "当前成员不在目标群内",
        )
    }

    async fn ticket(&self, agent: bool) -> Result<String> {
        let key = self.cache_key(if agent {
            "ticket:application"
        } else {
            "ticket:enterprise"
        });
        let token = self.token().await?;
        self.repository
            .cached_or_fetch(&key, || async {
                let path = if agent {
                    "ticket/get"
                } else {
                    "get_jsapi_ticket"
                };
                let mut req = self
                    .client
                    .get(self.endpoint(path))
                    .query(&[("access_token", token.as_str())]);
                if agent {
                    req = req.query(&[("type", "agent_config")]);
                }
                let v = self.json(req).await?;
                Ok((
                    v["ticket"].as_str().ok_or(Error::Unavailable)?.to_owned(),
                    v["expires_in"].as_i64().unwrap_or(7200) - 300,
                ))
            })
            .await
    }
    pub async fn signature(
        &self,
        a: &Actor,
        url: &str,
        origin: &str,
        agent: bool,
    ) -> Result<Value> {
        require(a.wecom_authenticated, "请在企业微信登录")?;
        let parsed = reqwest::Url::parse(url).map_err(|_| Error::Invalid("签名地址无效".into()))?;
        require(
            parsed.origin().ascii_serialization() == origin
                && parsed.path() == "/wecom"
                && parsed.fragment().is_none(),
            "签名地址不受信任",
        )?;
        let ticket = self.ticket(agent).await?;
        let nonce = secret();
        let timestamp = chrono::Utc::now().timestamp();
        let signature = sdk_signature(&ticket, &nonce, timestamp, url);
        Ok(json!({"timestamp":timestamp,"nonceStr":nonce,"signature":signature}))
    }
}

impl Service {
    pub fn directory(&self) -> super::directory::Service {
        super::directory::Service {
            store: crate::integrations::south::adapter::directory_postgres::Repository(
                self.repository.0.clone(),
            ),
            client: crate::integrations::south::adapter::directory_http::Client(self.clone()),
            corp: self.corp.clone(),
            credential_scope: self.credential_scope(),
            server_configured: self.server_configured(),
        }
    }
}

#[derive(serde::Serialize)]
pub struct WecomStatus {
    pub server_configured: bool,
    pub client_configured: bool,
    pub oauth_available: bool,
    pub share_available: bool,
}

#[cfg(test)]
#[path = "local_tests.rs"]
mod tests;
