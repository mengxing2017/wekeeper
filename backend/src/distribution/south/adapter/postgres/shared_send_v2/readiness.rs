//! Planning remains possible while device conditions are waiting.
use super::*;
pub(super) async fn resources(tx: &mut Tx<'_>) -> Result<Vec<Value>> {
    let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(b)||jsonb_build_object('client_kind',t.client_kind,'device_name',d.name,'target_name',t.search_text,'search_text',t.search_text,'online',coalesce(cp.state IN ('online','suspect'),false),'environment',e.observation) FROM mobile_device_target_bindings b JOIN mobile_targets t ON t.id=b.target_id JOIN execution_devices d ON d.id=b.device_id LEFT JOIN device_control_presence cp ON cp.id=d.id LEFT JOIN mobile_device_environments e ON e.device_id=d.id AND e.credential_generation=d.credential_generation ORDER BY d.name,b.id").fetch_all(&mut **tx).await?;
    let mut result = Vec::new();
    for mut row in rows {
        row["resource_allowed"] = json!(resource_budgets::check(tx, uid(&row, "id")?).await?);
        let candidate = tasks::raw_candidates(tx, uid(&row, "target_id")?)
            .await?
            .into_iter()
            .find(|c| c["device_id"] == row["device_id"]);
        let selectable = candidate.is_some();
        let confirmable = if let Some(ref c) = candidate {
            tasks::validate_plan(tx, c).await.is_ok()
        } else {
            false
        };
        let busy = inspections::busy(tx, uid(&row, "device_id")?).await?;
        let mut blockers = Vec::new();
        let mut add = |code: &str, message: &str, owner: &str, action: &str, automatic: bool| {
            blockers.push(json!({"code":code,"message":message,"owner":owner,"action":action,"automatic":automatic,"observed_at":chrono::Utc::now()}))
        };
        if !selectable {
            add(
                "resource_unavailable",
                "绑定、手机、群或共享资格已停用",
                "platform",
                "检查共享资格和群绑定",
                false,
            );
        }
        let execution_conditions = execution_conditions(&row, busy, None);
        let ready = selectable
            && !busy
            && confirmable
            && row["online"] == true
            && row["resource_allowed"] == true;
        result.push(json!({"client_kind":row["client_kind"],"binding_id":row["id"],"device_id":row["device_id"],"target_id":row["target_id"],"device_name":row["device_name"],"target_name":row["target_name"],"search_text":row["search_text"],"target":candidate.as_ref().map(|c|c["target"].clone()),"plan_selectable":selectable,"confirmable":confirmable,"execution_ready":ready,"ready":ready,"state":if ready{json!("ready_for_task")}else{blockers.first().or_else(||execution_conditions.first()).map(|b|b["code"].clone()).unwrap_or(json!("waiting"))},"online":row["online"],"environment":row["environment"],"execution_conditions":execution_conditions,"blockers":blockers,"automatic":true}));
    }
    Ok(result)
}
pub(super) async fn pool(tx: &mut Tx<'_>) -> Result<Vec<Value>> {
    sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name) FROM execution_devices WHERE enabled ORDER BY name,id").fetch_all(&mut **tx).await.map_err(Into::into)
}

fn condition(code: &str, message: &str, owner: &str, action: &str, automatic: bool) -> Value {
    json!({"code":code,"message":message,"owner":owner,"action":action,"automatic":automatic,"observed_at":chrono::Utc::now()})
}
fn execution_conditions(row: &Value, busy: bool, mode: Option<&str>) -> Vec<Value> {
    let mut conditions = Vec::new();
    if mode.is_some_and(|m| m != "enabled") {
        conditions.push(condition(
            "execution_paused",
            "平台暂停实际执行，任务仍可提交排队",
            "platform",
            "等待平台恢复执行",
            true,
        ));
    }
    if row["resource_allowed"] == false {
        conditions.push(condition(
            "resource_budget_unavailable",
            "候选绑定累计额度不足或有效期已结束，平台将选择其他授权候选",
            "admin",
            "核对真实绑定约束",
            false,
        ));
    }
    if row["online"] != true {
        conditions.push(condition(
            "waiting_connection",
            "手机当前离线，平台将选择下一授权候选",
            "system",
            "平台自动切换可用候选",
            true,
        ));
    }
    if busy {
        conditions.push(condition(
            "waiting_physical_owner",
            "手机正在处理原工作，当前任务可以提交排队",
            "system",
            "等待原工作结束",
            true,
        ));
    }
    conditions
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_is_optional_and_never_a_delivery_condition() {
        let mut row = json!({"online":true});
        assert!(execution_conditions(&row, false, Some("enabled")).is_empty());
        row["environment"] =
            json!({"observed_at":"2000-01-01T00:00:00Z","accessibility":"no","interactive":"no"});
        assert!(execution_conditions(&row, false, Some("enabled")).is_empty());
        assert_eq!(
            execution_conditions(&row, true, Some("enabled"))[0]["code"],
            "waiting_physical_owner"
        );
        assert_eq!(
            execution_conditions(&row, false, Some("disabled"))[0]["code"],
            "execution_paused"
        );
        row["online"] = json!(false);
        assert_eq!(
            execution_conditions(&row, false, Some("enabled"))[0]["code"],
            "waiting_connection"
        );
    }
}
