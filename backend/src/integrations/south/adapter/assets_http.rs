use crate::integrations::{
    domain::{assets::*, directory::ReadError},
    south::{adapter::directory_http::Client, port::assets::AssetClient},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
fn decode<T: DeserializeOwned>(v: Value) -> Result<T, ReadError> {
    serde_json::from_value(v).map_err(|_| ReadError::Invalid)
}
impl AssetClient for Client {
    async fn groups(&self, owners: &[String], cursor: &str) -> Result<GroupPage, ReadError> {
        decode(self.request("externalcontact/groupchat/list",&[],Some(json!({"status_filter":0,"owner_filter":{"userid_list":owners},"cursor":cursor,"limit":100}))).await?)
    }
    async fn group(&self, id: &str) -> Result<Group, ReadError> {
        let v = self
            .request(
                "externalcontact/groupchat/get",
                &[],
                Some(json!({"chat_id":id,"need_name":0})),
            )
            .await?;
        let g: Group = decode(v["group_chat"].clone())?;
        g.validate(id)?;
        Ok(g)
    }
}
