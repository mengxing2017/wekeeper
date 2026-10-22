use crate::{
    business::{Error, Result, require},
    distribution::{domain::broadcast::*, south::port::broadcast::Client as Port},
    integrations::{
        north::local::Service,
        south::{adapter::directory_http, port::assets::AssetClient},
    },
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
#[derive(Clone)]
pub struct Client(pub Service);
impl Client {
    // Reads may be retried. This method must NEVER be used for external writes.
    async fn pages(&self, endpoint: &str, mut body: Value, field: &str) -> Result<Vec<Value>> {
        let reader = directory_http::Client(self.0.clone());
        let mut cursors = BTreeSet::new();
        let mut rows = Vec::new();
        for _ in 0..1000 {
            let v = reader
                .request(endpoint, &[], Some(body.clone()))
                .await
                .map_err(|_| Error::Unavailable)?;
            rows.extend(
                v[field]
                    .as_array()
                    .ok_or(Error::Unavailable)?
                    .iter()
                    .cloned(),
            );
            let next = v["next_cursor"].as_str().ok_or(Error::Unavailable)?;
            if next.is_empty() {
                return Ok(rows);
            }
            require(cursors.insert(next.to_owned()), "平台分页游标循环")?;
            body["cursor"] = next.into();
        }
        Err(Error::Unavailable)
    }
}
impl Port for Client {
    async fn generation(&self) -> Result<(String, i64)> {
        Ok((
            self.0.corp.clone(),
            self.0.directory().ensure_config().await?,
        ))
    }
    async fn verify(&self, w: &Work) -> Result<()> {
        let (corp, generation) = self.generation().await?;
        require(
            corp == w.corp_id && generation == w.generation,
            "凭据代次已变化",
        )?;
        if w.operation != "create" {
            return Ok(());
        }
        require(
            !w.chats.is_empty() && w.chats.len() <= 2000,
            "客户群目标无效",
        )?;
        self.0.directory().verify_account(w.sender_account).await?;
        let reader = directory_http::Client(self.0.clone());
        for chat in &w.chats {
            let group = reader.group(chat).await.map_err(|_| Error::Unavailable)?;
            require(
                group.owner.eq_ignore_ascii_case(&w.sender),
                "群主已变化，请重新预览任务",
            )?;
        }
        // Fetch/refresh token before recording a one-shot invocation.
        self.0.token().await?;
        Ok(())
    }
    async fn invoke(&self, w: &Work) -> Outcome {
        let Ok(token) = self.0.token().await else {
            return Outcome::Unknown;
        };
        let (endpoint, body) = match w.operation.as_str() {
            "create" if !w.chats.is_empty() && w.chats.len() <= 2000 => (
                "add_msg_template",
                json!({"chat_type":"group","sender":w.sender,"chat_id_list":w.chats,"text":{"content":w.content}}),
            ),
            "remind" => ("remind_groupmsg_send", json!({"msgid":w.msgid})),
            "stop" => ("cancel_groupmsg_send", json!({"msgid":w.msgid})),
            _ => return Outcome::Rejected("invalid_operation".into()),
        };
        // Dedicated client: no HTTP retry or redirect can repeat a POST after a lost response.
        let Ok(client) = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .build()
        else {
            return Outcome::Unknown;
        };
        let response = client
            .post(self.0.endpoint(&format!("externalcontact/{endpoint}")))
            .query(&[("access_token", token)])
            .json(&body)
            .send()
            .await;
        let Ok(response) = response else {
            return Outcome::Unknown;
        };
        if !response.status().is_success() {
            return Outcome::Unknown;
        }
        let Ok(v) = response.json::<Value>().await else {
            return Outcome::Unknown;
        };
        if w.operation == "create" {
            return created_response(v, &w.chats);
        }
        match v["errcode"].as_i64() {
            Some(0) => Outcome::Done,
            Some(code) => Outcome::Rejected(code.to_string()),
            None => Outcome::Unknown,
        }
    }
    async fn results(&self, w: &Work) -> Result<Results> {
        self.verify(&Work {
            operation: "read".into(),
            ..w.clone()
        })
        .await?;
        let msgid = w.msgid.as_ref().ok_or(Error::Conflict)?;
        let members = self
            .pages(
                "externalcontact/get_groupmsg_task",
                json!({"msgid":msgid,"limit":1000}),
                "task_list",
            )
            .await?;
        let mut member_status = None;
        for m in members {
            let userid = m["userid"].as_str().ok_or(Error::Unavailable)?;
            require(
                userid.eq_ignore_ascii_case(&w.sender),
                "平台执行成员与任务不匹配",
            )?;
            let status = m["status"]
                .as_i64()
                .and_then(|v| i32::try_from(v).ok())
                .ok_or(Error::Unavailable)?;
            if let Some(old) = member_status {
                require(old == status, "成员结果冲突")?;
            }
            member_status = Some(status);
        }
        let groups = self
            .pages(
                "externalcontact/get_groupmsg_send_result",
                json!({"msgid":msgid,"userid":w.sender,"limit":1000}),
                "send_list",
            )
            .await?;
        let mut observations = std::collections::BTreeMap::new();
        for v in groups {
            require(
                v["userid"]
                    .as_str()
                    .is_some_and(|u| u.eq_ignore_ascii_case(&w.sender)),
                "平台结果成员不匹配",
            )?;
            let observation: Observation =
                serde_json::from_value(v).map_err(|_| Error::Unavailable)?;
            require(
                w.chats.contains(&observation.chat_id),
                "平台返回目标范围之外的群，请核对",
            )?;
            if let Some(old) = observations.insert(observation.chat_id.clone(), observation.clone())
            {
                require(
                    old.status == observation.status && old.send_time == observation.send_time,
                    "平台返回冲突结果",
                )?;
            }
        }
        Ok(Results {
            member_status,
            groups: observations.into_values().collect(),
        })
    }
    async fn candidates(&self, start: i64, end: i64) -> Result<Vec<Value>> {
        require(
            start > 0 && end >= start && end - start <= 28 * 86400,
            "核对时间窗口须在 28 天内",
        )?;
        self.pages("externalcontact/get_groupmsg_list_v2",json!({"chat_type":"group","filter_type":0,"start_time":start,"end_time":end,"limit":100}),"group_msg_list").await
    }
}
