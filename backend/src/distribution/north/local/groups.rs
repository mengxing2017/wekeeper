use crate::{
    business::{Actor, Result, require, text},
    distribution::south::adapter::postgres::groups::Repository,
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service(pub Repository);
impl Service {
    pub async fn group(
        &self,
        a: &Actor,
        school: Uuid,
        mut v: Value,
        integration: &crate::integrations::north::local::Service,
    ) -> Result<Value> {
        if school.is_nil() {
            a.platform()?;
        } else {
            a.authorize(school, "admin")?;
        }
        if v["revision"].as_i64().unwrap_or(0) == 0 {
            a.platform()?;
        }
        for k in ["corp_id", "chat_id", "name"] {
            text(v[k].as_str().unwrap_or(""), 1, 200)?;
        }
        // Revocation remains possible during platform outages or credential changes.
        let active = v["active"].as_bool().unwrap_or(true);
        v.as_object_mut()
            .ok_or(crate::business::Error::Conflict)?
            .remove("directory_group_id");
        v.as_object_mut().unwrap().remove("verified_generation");
        if active || v["revision"].as_i64().unwrap_or(0) == 0 {
            require(
                v["corp_id"].as_str() == Some(&integration.corp),
                "企业 ID 必须与当前应用配置一致",
            )?;
            use crate::integrations::south::port::assets::AssetClient;
            let generation = integration.directory().ensure_config().await?;
            let at = chrono::Utc::now();
            let assets = integration.assets();
            let group = assets
                .client
                .group(v["chat_id"].as_str().unwrap_or(""))
                .await
                .map_err(|_| crate::business::Error::Unavailable)?;
            let id = assets.store.observe(generation, &group, at).await?;
            v["directory_group_id"] = serde_json::json!(id);
            v["verified_generation"] = serde_json::json!(generation);
        }
        self.0.group(a, school, v).await
    }
    pub async fn groups(&self, a: &Actor, s: Uuid) -> Result<Vec<Value>> {
        a.authorize(s, "operator")?;
        self.0.groups(s).await
    }
}
impl crate::bootstrap::Application {
    pub fn distribution_groups(&self) -> Service {
        Service(Repository(self.pool.clone()))
    }
}
