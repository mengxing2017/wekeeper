use super::*;

async fn snapshot(tx: &mut Tx<'_>, school: Uuid, v: &TaskPreviewInput) -> Result<Value> {
    v.validate(100)?;
    let configuration:Value=sqlx::query_scalar("SELECT jsonb_build_object('device_id',d.id,'credential_generation',d.credential_generation,'grant_id',g.id,'grant_revision',g.revision,'consent_id',c.id,'consent_revision',c.revision,'assignment_id',a.id,'assignment_revision',a.execution_revision,'release_id',r.id,'release_revision',r.revision,'flow_version_id',f.id,'flow_digest',f.content_digest,'flow',f.flow) FROM mobile_execution_assignments a JOIN execution_devices d ON d.id=a.device_id JOIN mobile_device_school_grants g ON g.id=a.grant_id JOIN mobile_device_consents c ON c.grant_id=g.id JOIN mobile_execution_flow_releases r ON r.school_id=a.school_id AND r.id=a.release_id JOIN mobile_execution_flow_versions f ON f.school_id=r.school_id AND f.id=r.flow_version_id WHERE a.school_id=$1 AND a.device_id=$2 AND a.enabled AND d.enabled AND g.enabled AND r.enabled AND c.accepted AND c.grant_revision=g.revision AND c.scope->>'purpose'=g.purpose AND EXISTS(SELECT 1 FROM mobile_execution_flow_validations v WHERE v.school_id=a.school_id AND v.device_id=a.device_id AND v.flow_version_id=f.id AND v.state='passed' AND v.grant_revision=g.revision AND v.consent_id=c.id AND v.consent_revision=c.revision AND v.credential_generation=d.credential_generation AND NOT EXISTS(SELECT 1 FROM mobile_execution_flow_validations newer WHERE newer.device_id=v.device_id AND newer.flow_version_id=v.flow_version_id AND newer.state<>'pending' AND newer.completed_at>v.completed_at)) FOR SHARE OF a,d,g,c,r,f")
        .bind(school).bind(v.device_id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("mobile_execution_configuration_required"))?;
    let flow: FlowV2 =
        serde_json::from_value(configuration["flow"].clone()).map_err(|_| Error::Conflict)?;
    flow.validate()?;
    v.validate(flow.max_targets)?;
    let mut items = Vec::new();
    for (position, target) in v.target_ids.iter().enumerate() {
        let item:Value=sqlx::query_scalar("SELECT jsonb_build_object('target_id',t.id,'search_text',t.search_text,'locator_revision',t.locator_revision,'binding_id',b.id,'binding_revision',b.revision) FROM mobile_targets t JOIN mobile_device_target_bindings b ON b.school_id=t.school_id AND b.target_id=t.id JOIN mobile_device_school_grants g ON g.id=b.grant_id WHERE t.school_id=$1 AND t.id=$2 AND b.device_id=$3 AND t.enabled AND t.archived_at IS NULL AND b.enabled AND g.enabled FOR SHARE OF t,b,g")
            .bind(school).bind(target).bind(v.device_id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("mobile_target_binding_required"))?;
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_task_items WHERE school_id=$1 AND target_id=$2 AND state IN ('pending','claimed','prepared','invocation_issued','unknown'))")
            .bind(school).bind(target).fetch_one(&mut **tx).await?;
        if pending {
            return Err(Error::Policy("mobile_target_unresolved"));
        }
        let mut item = item;
        item["position"] = json!(position + 1);
        items.push(item);
    }
    Ok(
        json!({"protocol_version":6,"school_id":school,"configuration":configuration,"message":v.message,"selection_policy":SELECTION_POLICY,"items":items}),
    )
}

pub(super) async fn write(
    tx: &mut Tx<'_>,
    school: Uuid,
    actor: Uuid,
    action: &str,
    id: Uuid,
    input: &Value,
) -> Result<Value> {
    match action {
        "task-preview" => {
            let v: TaskPreviewInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("请选择设备、目标并填写消息".into()))?;
            let snapshot = snapshot(tx, school, &v).await?;
            let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_device_reservations WHERE device_id=$1)").bind(v.device_id).fetch_one(&mut **tx).await?;
            if occupied {
                return Err(Error::Policy("mobile_device_reserved"));
            }
            sqlx::query("INSERT INTO mobile_execution_task_previews(id,school_id,device_id,input,snapshot,digest,created_by) VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(id).bind(school).bind(v.device_id).bind(input).bind(&snapshot).bind(hash(&snapshot.to_string())).bind(actor).execute(&mut **tx).await?;
            object(tx, "mobile_execution_task_previews", id).await
        }
        "confirm-task" => {
            let v: TaskConfirmationInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("确认字段无效".into()))?;
            v.validate()?;
            let preview:Value=sqlx::query_scalar("SELECT to_jsonb(p) FROM mobile_execution_task_previews p WHERE school_id=$1 AND id=$2 AND expires_at>clock_timestamp() FOR UPDATE")
                .bind(school).bind(v.preview_id).fetch_optional(&mut **tx).await?.ok_or(Error::Policy("mobile_preview_expired"))?;
            if preview["digest"] != v.preview_digest {
                return Err(Error::Policy("revision_conflict"));
            }
            let consumed: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM mobile_execution_tasks WHERE preview_id=$1)",
            )
            .bind(v.preview_id)
            .fetch_one(&mut **tx)
            .await?;
            if consumed {
                return Err(Error::Conflict);
            }
            let request: TaskPreviewInput =
                serde_json::from_value(preview["input"].clone()).map_err(|_| Error::Conflict)?;
            let current = snapshot(tx, school, &request).await?;
            if current != preview["snapshot"] || hash(&current.to_string()) != v.preview_digest {
                return Err(Error::Policy("revision_conflict"));
            }
            let occupied:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM mobile_execution_device_reservations WHERE device_id=$1)").bind(request.device_id).fetch_one(&mut **tx).await?;
            if occupied {
                return Err(Error::Policy("mobile_device_reserved"));
            }
            let creator: Uuid = serde_json::from_value(preview["created_by"].clone())
                .map_err(|_| Error::Conflict)?;
            let intent = Uuid::now_v7();
            sqlx::query("INSERT INTO mobile_execution_tasks(id,school_id,intent_id,device_id,preview_id,created_by,confirmed_by) VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(id).bind(school).bind(intent).bind(request.device_id).bind(v.preview_id).bind(creator).bind(actor).execute(&mut **tx).await?;
            sqlx::query("INSERT INTO mobile_execution_plans(id,school_id,task_id,snapshot,digest) VALUES($1,$2,$3,$4,$5)").bind(Uuid::now_v7()).bind(school).bind(id).bind(&current).bind(&v.preview_digest).execute(&mut **tx).await?;
            for item in current["items"].as_array().ok_or(Error::Conflict)? {
                let target: Uuid = serde_json::from_value(item["target_id"].clone())
                    .map_err(|_| Error::Conflict)?;
                let binding: Uuid = serde_json::from_value(item["binding_id"].clone())
                    .map_err(|_| Error::Conflict)?;
                sqlx::query("INSERT INTO mobile_execution_task_items(id,school_id,task_id,intent_id,device_id,target_id,binding_id,position,snapshot) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                    .bind(Uuid::now_v7()).bind(school).bind(id).bind(intent).bind(request.device_id).bind(target).bind(binding).bind(item["position"].as_i64().ok_or(Error::Conflict)? as i32).bind(item).execute(&mut **tx).await?;
            }
            sqlx::query("INSERT INTO mobile_execution_device_reservations(device_id,school_id,task_id) VALUES($1,$2,$3)").bind(request.device_id).bind(school).bind(id).execute(&mut **tx).await?;
            object(tx, "mobile_execution_tasks", id).await
        }
        "cancel-task" => {
            let v: RevisionInput = serde_json::from_value(input.clone())
                .map_err(|_| Error::Invalid("取消字段无效".into()))?;
            validate_text(&v.reason, 1, 2000, 8000)?;
            // Only certainly unclaimed queued work may release a reservation here.
            let n=sqlx::query("UPDATE mobile_execution_tasks t SET state='cancelled',revision=revision+1,cancelled_at=clock_timestamp() WHERE school_id=$1 AND id=$2 AND revision=$3 AND state='queued' AND NOT EXISTS(SELECT 1 FROM mobile_execution_task_items i WHERE i.task_id=t.id AND i.state<>'pending')")
                .bind(school).bind(id).bind(v.expected_revision).execute(&mut **tx).await?.rows_affected();
            if n != 1 {
                return Err(Error::Policy("mobile_task_not_cancellable"));
            }
            sqlx::query("UPDATE mobile_execution_task_items SET state='cancelled' WHERE school_id=$1 AND task_id=$2 AND state='pending'").bind(school).bind(id).execute(&mut **tx).await?;
            sqlx::query("DELETE FROM mobile_execution_device_reservations WHERE school_id=$1 AND task_id=$2").bind(school).bind(id).execute(&mut **tx).await?;
            object(tx, "mobile_execution_tasks", id).await
        }
        _ => Err(Error::NotFound),
    }
}
