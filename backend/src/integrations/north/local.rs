use super::super::south::adapter::postgres::Repository;
use crate::business::{Actor, Error, Result, require, secret};
use crate::integrations::domain::{group_contains_member, sdk_signature};
use serde_json::{Value, json};
#[derive(Clone)]
pub struct Service {
    pub repository: Repository,
    client: reqwest::Client,
    pub corp: String,
    pub agent: String,
    secret: String,
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
            wx_app: env("WEKEEPER_WX_APP_ID"),
            wx_secret: env("WEKEEPER_WX_SECRET"),
        }
    }
    pub fn configured(&self) -> bool {
        !self.corp.is_empty() && !self.agent.is_empty() && !self.secret.is_empty()
    }
    async fn json(&self, request: reqwest::RequestBuilder) -> Result<Value> {
        let response = request.send().await.map_err(|_| Error::Unavailable)?;
        if !response.status().is_success() {
            return Err(Error::Unavailable);
        }
        let v: Value = response.json().await.map_err(|_| Error::Unavailable)?;
        if v["errcode"].as_i64().unwrap_or(0) != 0 {
            return Err(Error::Unavailable);
        }
        Ok(v)
    }
    async fn token(&self) -> Result<String> {
        if !self.configured() {
            return Err(Error::Unavailable);
        }
        let key = format!("wecom:{}:{}:token", self.corp, self.agent);
        self.repository
            .cached_or_fetch(&key, || async {
                let v = self
                    .json(
                        self.client
                            .get("https://qyapi.weixin.qq.com/cgi-bin/gettoken")
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
    pub async fn oauth_member(&self, code: &str) -> Result<uuid::Uuid> {
        let token = self.token().await?;
        let v = self
            .json(
                self.client
                    .get("https://qyapi.weixin.qq.com/cgi-bin/auth/getuserinfo")
                    .query(&[("access_token", token.as_str()), ("code", code)]),
            )
            .await?;
        self.repository
            .member(
                v["userid"]
                    .as_str()
                    .or(v["UserId"].as_str())
                    .ok_or(Error::Forbidden)?,
            )
            .await
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
    pub async fn verify_group(&self, a: &Actor, corp: &str, chat: &str) -> Result<()> {
        require(a.wecom_authenticated && corp == self.corp, "企业身份不匹配")?;
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
        let key = format!("wecom:{}:{}:ticket:{}", self.corp, self.agent, agent);
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
                    .get(format!("https://qyapi.weixin.qq.com/cgi-bin/{path}"))
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
