//! One resolution rule for opening summaries and durable resource coordination.
use super::*;

pub(super) async fn resolve(tx: &mut Tx<'_>, school: Uuid, body: &mut Value) -> Result<()> {
    let devices: Vec<Uuid> =
        serde_json::from_value(body["device_ids"].clone()).map_err(|_| Error::Conflict)?;
    let targets = body["targets"].as_array().ok_or(Error::Conflict)?.clone();
    let mut locators = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        for device in &devices {
            let binding: Option<Value> = if let Some(id) = target["id"].as_str() {
                sqlx::query_scalar("SELECT to_jsonb(b) FROM mobile_device_target_bindings b WHERE b.school_id=$1 AND b.device_id=$2 AND b.target_id=$3")
                    .bind(school).bind(device).bind(id.parse::<Uuid>().map_err(|_| Error::Conflict)?).fetch_optional(&mut **tx).await?
            } else {
                None
            };
            require(
                binding.as_ref().is_none_or(|b| b["enabled"] == true),
                "目标绑定已停用，请先恢复绑定",
            )?;
            let name: String = sqlx::query_scalar("SELECT name FROM execution_devices WHERE id=$1")
                .bind(device)
                .fetch_one(&mut **tx)
                .await?;
            let over = binding
                .as_ref()
                .and_then(|b| b["search_text_override"].as_str());
            locators.push(json!({"device_id":device,"device_name":name,"target_id":target["id"],"target_index":index,"target_name":target["name"],"search_text":over.map(json_string).unwrap_or_else(||target["search_text"].clone()),"search_source":if over.is_some(){"override"}else{"default"},"binding_locator_revision":binding.as_ref().map(|b|b["locator_revision"].clone()).unwrap_or(Value::Null),"target_locator_revision":if over.is_some(){Value::Null}else{target.get("revision").cloned().unwrap_or(json!(1))}}));
        }
    }
    let locators: Vec<crate::distribution::domain::shared_mobile::usage::UsageLocator> =
        serde_json::from_value(json!(locators)).map_err(|_| Error::Conflict)?;
    body["locators"] = json!(locators);
    Ok(())
}
fn json_string(value: &str) -> Value {
    json!(value)
}

pub(super) fn comparison_body(value: &Value) -> Value {
    let mut body = value.clone();
    if let Some(o) = body.as_object_mut() {
        o.remove("scheme");
        o.remove("scheme_id");
        o.remove("renewal_of");
        o.remove("expected_revision");
    }
    let resolved = body["locators"].is_array();
    if let Some(targets) = body["targets"].as_array_mut() {
        for t in targets {
            if t["id"].is_string()
                && let Some(o) = t.as_object_mut()
            {
                o.remove("name");
                if resolved {
                    o.remove("search_text");
                    o.remove("revision");
                }
            }
        }
    }
    if let Some(locators) = body["locators"].as_array_mut() {
        for l in locators {
            if let Some(o) = l.as_object_mut() {
                o.remove("device_name");
                o.remove("target_name");
            }
        }
    }
    body
}

pub(super) fn same_bindings(a: &Value, b: &Value) -> bool {
    fn ids(v: &Value, key: &str) -> std::collections::BTreeSet<String> {
        v[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|x| {
                if key == "targets" {
                    x["id"].to_string()
                } else {
                    x.to_string()
                }
            })
            .collect()
    }
    ids(a, "device_ids") == ids(b, "device_ids")
        && ids(a, "targets") == ids(b, "targets")
        && a["auto_targets"] == b["auto_targets"]
}

pub(super) fn same_locator(a: &Value, b: &Value) -> bool {
    [
        "device_id",
        "target_id",
        "search_text",
        "search_source",
        "target_locator_revision",
    ]
    .iter()
    .all(|k| a[k] == b[k])
        && (a["binding_locator_revision"] == b["binding_locator_revision"]
            || (a["binding_locator_revision"].is_null() && b["binding_locator_revision"] == 1))
}
