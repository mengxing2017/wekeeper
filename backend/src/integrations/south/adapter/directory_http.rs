use crate::integrations::{
    domain::directory::*, north::local::Service, south::port::DirectoryClient,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
#[derive(Clone)]
pub struct Client(pub Service);
impl Client {
    async fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, ReadError> {
        self.request(path, query, None).await
    }
    pub(crate) async fn request(
        &self,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, ReadError> {
        let mut refreshed = false;
        for attempt in 0..3 {
            self.0
                .repository
                .directory_throttle()
                .await
                .map_err(|_| ReadError::Transient)?;
            let token = match self.0.token().await {
                Ok(token) => token,
                Err(crate::business::Error::Upstream(c)) if c != 45009 => {
                    tracing::warn!(
                        endpoint = "gettoken",
                        errcode = c,
                        "wecom read token request rejected"
                    );
                    return Err(ReadError::Rejected(c));
                }
                Err(_) if attempt < 2 => {
                    tokio::time::sleep(std::time::Duration::from_millis(500 * (1 << attempt)))
                        .await;
                    continue;
                }
                Err(_) => return Err(ReadError::Transient),
            };
            let request = if let Some(ref body) = body {
                self.0.client.post(self.0.endpoint(path)).json(body)
            } else {
                self.0.client.get(self.0.endpoint(path))
            };
            let response = request
                .query(&[("access_token", token.as_str())])
                .query(query)
                .send()
                .await;
            if let Ok(response) = response {
                if response.status().is_success() {
                    let v: Value = response.json().await.map_err(|_| ReadError::Invalid)?;
                    let code = match v.get("errcode") {
                        None => 0,
                        Some(c) => c.as_i64().ok_or(ReadError::Invalid)?,
                    };
                    if code == 0 {
                        return Ok(v);
                    }
                    if matches!(code, 40014 | 42001) && !refreshed {
                        self.0
                            .repository
                            .expire_token(&self.0.cache_key("token"), &token)
                            .await
                            .map_err(|_| ReadError::Transient)?;
                        refreshed = true;
                        continue;
                    }
                    if code != 45009 {
                        // Only the static API path: never log query parameters or upstream errmsg.
                        tracing::warn!(
                            endpoint = path,
                            errcode = code,
                            "wecom read API rejected request"
                        );
                        return Err(ReadError::Rejected(code));
                    }
                } else if !response.status().is_server_error() && response.status().as_u16() != 429
                {
                    return Err(ReadError::Invalid);
                }
            }
            if attempt < 2 {
                tokio::time::sleep(std::time::Duration::from_millis(
                    500 * (1 << attempt) + (uuid::Uuid::new_v4().as_u128() % 200) as u64,
                ))
                .await;
            }
        }
        Err(ReadError::Transient)
    }
    fn decode<T: DeserializeOwned>(v: Value) -> Result<T, ReadError> {
        serde_json::from_value(v).map_err(|_| ReadError::Invalid)
    }
}
impl DirectoryClient for Client {
    async fn departments(&self) -> Result<Vec<Department>, ReadError> {
        let v = self.get("department/simplelist", &[]).await?;
        let ids = v["department_id"].as_array().ok_or(ReadError::Invalid)?;
        let mut departments = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for d in ids {
            let id = d["id"]
                .as_i64()
                .filter(|id| *id > 0)
                .ok_or(ReadError::Invalid)?;
            if !seen.insert(id) {
                return Err(ReadError::Invalid);
            }
            let v = self
                .get("department/get", &[("id", id.to_string())])
                .await?;
            let d: Department = Self::decode(v["department"].clone())?;
            if d.id != id || d.name.trim().is_empty() {
                return Err(ReadError::Invalid);
            }
            departments.push(d);
        }
        Ok(departments)
    }
    async fn members(&self, id: i64) -> Result<Vec<Member>, ReadError> {
        let v = self
            .get("user/list", &[("department_id", id.to_string())])
            .await?;
        Self::decode(v["userlist"].clone())
    }
    async fn member(&self, userid: &str) -> Result<Member, ReadError> {
        let v = self.get("user/get", &[("userid", userid.into())]).await?;
        Self::decode(v)
    }
}
