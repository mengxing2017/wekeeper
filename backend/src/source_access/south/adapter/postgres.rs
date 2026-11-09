use crate::source_access::{crypto, domain::*};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;
pub struct Repository(pub PgPool);
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    source: Uuid,
    key: Option<&str>,
    actor: Option<Uuid>,
    action: &str,
    request: &str,
    detail: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO source_access_audit(id,source_id,key_id,actor_id,action,result_code,request_id,detail) VALUES($1,$2,$3,$4,$5,'ok',$6,$7)").bind(Uuid::now_v7()).bind(source).bind(key).bind(actor).bind(action).bind(request).bind(detail).execute(&mut **tx).await?;
    Ok(())
}
fn active(row: &sqlx::postgres::PgRow) -> Result<()> {
    ensure(
        credential_available(
            &row.get::<String, _>("status"),
            row.get("valid_from"),
            row.get("valid_until"),
            chrono::Utc::now(),
        ),
        "credential_unavailable",
    )
}
impl Repository {
    pub async fn caller_for_key(&self, key: &str) -> Result<Uuid> {
        sqlx::query_scalar("SELECT source_id FROM source_credentials WHERE key_id=$1")
            .bind(key)
            .fetch_optional(&self.0)
            .await?
            .ok_or(Fault("not_found"))
    }
    pub async fn machine(&self, s: Signed, body: Vec<u8>) -> Result<(bool, Value)> {
        let credential=sqlx::query("SELECT c.* FROM source_credentials c JOIN source_callers p USING(source_id) WHERE key_id=$1").bind(&s.key_id).fetch_optional(&self.0).await?.ok_or(Fault("authentication_failed"))?;
        active(&credential)?;
        let source: Uuid = credential.get("source_id");
        let material: Option<Vec<u8>> = credential.get("encrypted_secret");
        let material = crypto::open(&material.ok_or(Fault("credential_unavailable"))?, &s.key_id)?;
        verify(
            &secret(&material)?,
            &canonical(
                &s.key_id,
                &s.method,
                &s.path,
                &s.timestamp,
                &s.nonce.to_string(),
                &s.identity.to_string(),
                &body,
            ),
            &s.signature,
        )?;
        let seconds = window(&s.timestamp, chrono::Utc::now().timestamp())?;
        // Autocommit is intentional: failed DTO or business transactions cannot free a used nonce.
        let count=sqlx::query("INSERT INTO source_request_nonces(key_id,source_id,nonce,expires_at) VALUES($1,$2,$3,to_timestamp($4)) ON CONFLICT DO NOTHING").bind(&s.key_id).bind(source).bind(s.nonce).bind((seconds+360) as f64).execute(&self.0).await?.rows_affected();
        ensure(count == 1, "replay_detected")?;
        let mut tx = self.0.begin().await?;
        // Source lock serializes authorization changes and receipt creation for this platform.
        // Cleanup is bounded and independent of the lifetime of event identities.
        sqlx::query("DELETE FROM source_request_nonces WHERE (key_id,nonce) IN (SELECT key_id,nonce FROM source_request_nonces WHERE expires_at<now() LIMIT 100)").execute(&mut *tx).await?;
        let p = sqlx::query("SELECT * FROM source_callers WHERE source_id=$1 FOR UPDATE")
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
        let c=sqlx::query("SELECT c.* FROM source_credentials c JOIN source_callers p USING(source_id) WHERE key_id=$1 FOR UPDATE OF c").bind(&s.key_id).fetch_one(&mut *tx).await?;
        active(&c)?;
        ensure(
            c.get::<i64, _>("revision") == credential.get::<i64, _>("revision"),
            "credential_unavailable",
        )?;
        let requests:i64=sqlx::query_scalar("SELECT count(*) FROM source_request_nonces WHERE key_id=$1 AND created_at>now()-interval '1 minute'").bind(&s.key_id).fetch_one(&mut *tx).await?;
        ensure(requests <= 600, "rate_limited")?;
        let result = if s.path == "/api/v2/source-authentications"
            || s.path == "/api/v2/source-route-checks"
        {
            let doc: Document<Check> =
                serde_json::from_slice(&body).map_err(|_| Fault("invalid_request"))?;
            let kind = if s.path.ends_with("source-authentications") {
                "source-authentications"
            } else {
                "source-route-checks"
            };
            ensure(
                doc.data.kind == kind && doc.data.id == s.identity,
                "event_identity_mismatch",
            )?;
            let check = doc.data.attributes;
            ensure(
                check.route_id.as_deref().is_none_or(identifier),
                "invalid_request",
            )?;
            let limit = check.limit.unwrap_or(50);
            ensure((1..=100).contains(&limit), "invalid_request")?;
            let revision = p.get::<i64, _>("revision");
            let mut after = String::new();
            if let Some(cursor) = check.cursor {
                let value = String::from_utf8(decode(&cursor).map_err(|_| Fault("cursor_stale"))?)
                    .map_err(|_| Fault("cursor_stale"))?;
                let prefix = format!("{source}:{}:{revision}:", s.key_id);
                ensure(value.starts_with(&prefix), "cursor_stale")?;
                after = value[prefix.len()..].into();
            }
            let rows=sqlx::query("SELECT r.route_id,r.route_name,r.status,r.execution_state,r.retired_at FROM source_key_routes k JOIN source_routes r USING(route_id,source_id) WHERE k.key_id=$1 AND k.source_id=$2 AND k.status='active' AND r.route_id>$3 AND ($4::text IS NULL OR r.route_id=$4) ORDER BY r.route_id LIMIT $5").bind(&s.key_id).bind(source).bind(after).bind(&check.route_id).bind((limit+1) as i64).fetch_all(&mut *tx).await?;
            let next = if rows.len() > limit {
                Some(encode(
                    format!(
                        "{source}:{}:{revision}:{}",
                        s.key_id,
                        rows[limit - 1].get::<String, _>("route_id")
                    )
                    .as_bytes(),
                ))
            } else {
                None
            };
            let mut routes: Vec<crate::source_access::domain::routing::RouteCapability> = rows
                .iter()
                .take(limit)
                .map(|r| crate::source_access::domain::routing::RouteCapability {
                    route_id: r.get("route_id"),
                    available: r.get::<String, _>("status") == "active"
                        && r.get::<Option<chrono::DateTime<chrono::Utc>>, _>("retired_at")
                            .is_none(),
                    display_name: Some(r.get("route_name")),
                    execution_state: Some(r.get("execution_state")),
                })
                .collect();
            if routes.is_empty()
                && let Some(route) = check.route_id
            {
                routes.push(crate::source_access::domain::routing::RouteCapability {
                    route_id: route,
                    available: false,
                    display_name: None,
                    execution_state: None,
                });
            }
            (
                false,
                json!({"data":{"type":kind,"id":s.identity,"attributes":{"source_id":source,"signature_versions":[2],"schema_versions":[2],"paths":{"authentication":"/api/v2/source-authentications","events":"/api/v2/source-events","receipts":"/api/v2/source-receipts/{id}","routes":"/api/v2/source-route-checks"},"authorization_revision":revision,"credential_revision":c.get::<i64,_>("revision"),"routes":routes,"next_cursor":next}}}),
            )
        } else if s.method == "POST" {
            let doc: Document<Event> =
                serde_json::from_slice(&body).map_err(|_| Fault("forbidden_field"))?;
            ensure(
                doc.data.kind == "source-events"
                    && doc.data.id == s.identity
                    && doc.data.attributes.event_id == s.identity,
                "event_identity_mismatch",
            )?;
            let e = doc.data.attributes;
            e.validate()?;
            // Check idempotency before current route lifecycle, while keeping source ownership.
            let hash = digest(&body);
            let previous:Option<(Uuid,String)>=sqlx::query_as("SELECT receipt_id,body_sha256 FROM source_receipts WHERE source_id=$1 AND event_id=$2").bind(source).bind(s.identity).fetch_optional(&mut *tx).await?;
            if let Some((id, old_hash)) = previous {
                self.route(&mut tx, source, &s.key_id, &e.route_id, false)
                    .await?;
                ensure(old_hash == hash, "event_conflict")?;
                let value = self.receipt(&mut tx, source, id).await?;
                audit(
                    &mut tx,
                    source,
                    Some(&s.key_id),
                    None,
                    "source.retry",
                    &s.request_id,
                    json!({"event_id":s.identity}),
                )
                .await?;
                tx.commit().await?;
                return Ok((false, value));
            }
            let retired:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id=$1 AND source_id=$2)").bind(&e.route_id).bind(source).fetch_one(&mut *tx).await?;
            if retired && e.event_type == "content.withdrawn" {
                self.route(&mut tx, source, &s.key_id, &e.route_id, false)
                    .await?;
                let historical:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_inbox i JOIN source_receipts r USING(receipt_id) WHERE r.source_id=$1 AND r.route_id=$2 AND convert_from(i.request_body,'UTF8')::jsonb #>> '{data,attributes,external_content_id}'=$3)").bind(source).bind(&e.route_id).bind(&e.external_content_id).fetch_one(&mut *tx).await?;
                ensure(historical, "route_unavailable")?;
            } else {
                self.route(&mut tx, source, &s.key_id, &e.route_id, true)
                    .await?;
            }
            let hash = digest(&body);
            let old:Option<(Uuid,String)>=sqlx::query_as("SELECT receipt_id,body_sha256 FROM source_receipts WHERE source_id=$1 AND event_id=$2").bind(source).bind(s.identity).fetch_optional(&mut *tx).await?;
            let (id, fresh) = if let Some((id, old_hash)) = old {
                ensure(old_hash == hash, "event_conflict")?;
                (id, false)
            } else {
                let id = Uuid::now_v7();
                sqlx::query("INSERT INTO source_receipts(receipt_id,source_id,event_id,route_id,body_sha256,schema_version) VALUES($1,$2,$3,$4,$5,2)").bind(id).bind(source).bind(s.identity).bind(&e.route_id).bind(hash).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO source_inbox(receipt_id,request_body) VALUES($1,$2)")
                    .bind(id)
                    .bind(&body)
                    .execute(&mut *tx)
                    .await?;
                (id, true)
            };
            (fresh, self.receipt(&mut tx, source, id).await?)
        } else {
            ensure(body.is_empty(), "invalid_request")?;
            let id = s
                .path
                .rsplit('/')
                .next()
                .and_then(|v| v.parse::<Uuid>().ok())
                .ok_or(Fault("receipt_not_found"))?;
            let r=sqlx::query("SELECT route_id,event_id FROM source_receipts WHERE receipt_id=$1 AND source_id=$2").bind(id).bind(source).fetch_optional(&mut *tx).await?.ok_or(Fault("receipt_not_found"))?;
            self.route(
                &mut tx,
                source,
                &s.key_id,
                &r.get::<String, _>("route_id"),
                false,
            )
            .await
            .map_err(|_| Fault("receipt_not_found"))?;
            ensure(
                r.get::<Uuid, _>("event_id") == s.identity,
                "event_identity_mismatch",
            )?;
            (false, self.receipt(&mut tx, source, id).await?)
        };
        sqlx::query("UPDATE source_credentials SET last_used_at=now() WHERE key_id=$1")
            .bind(&s.key_id)
            .execute(&mut *tx)
            .await?;
        audit(&mut tx,source,Some(&s.key_id),None,"source.request",&s.request_id,json!({"signature_version":2,"schema_version":2,"credential_revision":c.get::<i64,_>("revision"),"authorization_revision":p.get::<i64,_>("revision"),"path":s.path})).await?;
        tx.commit().await?;
        Ok(result)
    }
    async fn route(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        source: Uuid,
        key: &str,
        route: &str,
        submit: bool,
    ) -> Result<()> {
        let allowed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_key_routes k JOIN source_routes r USING(route_id,source_id) WHERE k.key_id=$1 AND k.route_id=$2 AND k.source_id=$3 AND k.status='active' AND (NOT $4 OR (r.status='active' AND r.retired_at IS NULL)))").bind(key).bind(route).bind(source).bind(submit).fetch_one(&mut **tx).await?;
        ensure(allowed, "route_unavailable")
    }
    async fn receipt(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        source: Uuid,
        id: Uuid,
    ) -> Result<Value> {
        let value:Value=sqlx::query_scalar("SELECT jsonb_build_object('event_id',event_id,'status',status,'reason_code',reason_code,'processing_stage',processing_stage,'received_at',received_at,'updated_at',updated_at,'retryable',false,'sending',sending) FROM source_receipts WHERE source_id=$1 AND receipt_id=$2").bind(source).bind(id).fetch_one(&mut **tx).await?;
        Ok(json!({"data":{"type":"source-receipts","id":id,"attributes":value}}))
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn manage(
        &self,
        a: &crate::business::Actor,
        kind: &str,
        id: Option<&str>,
        method: &str,
        input: Option<AdminInput>,
        page: i64,
        search: &str,
        status: &str,
        filter_source: Option<Uuid>,
        validity: (
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
        ),
    ) -> Result<Value> {
        ensure(a.platform_admin, "forbidden")?;
        ensure((1..=100_000).contains(&page), "invalid_request")?;
        let table = match kind {
            "source-credentials" => "source_credentials",
            "source-routes" => "source_routes",
            _ => return Err(Fault("invalid_request")),
        };
        let pk = match kind {
            "source-credentials" => "key_id",
            _ => "route_id",
        };
        let mut tx = self.0.begin().await?;
        if method == "GET" {
            let owner_key = if kind == "source-credentials" {
                "t.key_id"
            } else {
                "(SELECT c.key_id FROM source_credentials c WHERE c.source_id=t.source_id ORDER BY c.key_id LIMIT 1)"
            };
            let query = format!(
                "SELECT to_jsonb(t) - 'encrypted_secret' - 'delivery_pending' - 'source_id' || jsonb_build_object('id',t.{pk},'key_id',{owner_key}) FROM {table} t JOIN source_callers p USING(source_id) WHERE ($1::text IS NULL OR {pk}::text=$1) AND ($2='' OR to_jsonb(t)->>'name' ILIKE $2 OR to_jsonb(t)->>'unit_name' ILIKE $2 OR to_jsonb(t)->>'route_name' ILIKE $2 OR {pk}::text ILIKE $2) AND ($3='' OR t.status=$3) AND ($4::uuid IS NULL OR source_id=$4) AND ($6::timestamptz IS NULL OR (to_jsonb(t)->>'valid_from')::timestamptz<=$6) AND ($7::timestamptz IS NULL OR (to_jsonb(t)->>'valid_until')::timestamptz<=$7) ORDER BY t.{pk} LIMIT 31 OFFSET $5"
            );
            let pattern = if search.is_empty() {
                String::new()
            } else {
                format!("%{search}%")
            };
            let rows: Vec<Value> = sqlx::query_scalar(&query)
                .bind(id)
                .bind(pattern)
                .bind(status)
                .bind(filter_source)
                .bind((page - 1) * 30)
                .bind(validity.0)
                .bind(validity.1)
                .fetch_all(&mut *tx)
                .await?;
            let audit_source = if let Some(source) = filter_source {
                Some(source)
            } else if let Some(key) = rows.first().and_then(|v| v["key_id"].as_str()) {
                Some(
                    sqlx::query_scalar("SELECT source_id FROM source_credentials WHERE key_id=$1")
                        .bind(key)
                        .fetch_optional(&mut *tx)
                        .await?
                        .ok_or(Fault("not_found"))?,
                )
            } else {
                None
            };
            if let Some(source) = audit_source {
                audit(
                    &mut tx,
                    source,
                    None,
                    Some(a.id),
                    "source.management.read",
                    &a.request_id,
                    json!({"kind":kind}),
                )
                .await?;
            }
            tx.commit().await?;
            let more = rows.len() > 30;
            return Ok(
                json!({"items":rows.into_iter().take(30).collect::<Vec<_>>(),"has_more":more}),
            );
        }
        let v = input.ok_or(Fault("invalid_request"))?;
        let existing_source = if let Some(id) = id {
            sqlx::query_scalar::<_, Uuid>(&format!(
                "SELECT source_id FROM {table} WHERE {pk}::text=$1"
            ))
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            None
        };
        let requested_source = if let Some(key) = v.key_id.as_deref() {
            Some(
                sqlx::query_scalar("SELECT source_id FROM source_credentials WHERE key_id=$1")
                    .bind(key)
                    .fetch_optional(&mut *tx)
                    .await?
                    .ok_or(Fault("not_found"))?,
            )
        } else {
            None
        };
        // A new credential owns a new internal identity; only key_id is exposed.
        let creating_caller = kind == "source-credentials" && id.is_none();
        let source = if creating_caller {
            ensure(v.key_id.is_none(), "invalid_request")?;
            Uuid::now_v7()
        } else {
            existing_source
                .or(requested_source)
                .ok_or(Fault("invalid_request"))?
        };
        if !creating_caller {
            sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
                .bind(source)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Fault("invalid_request"))?;
        }
        if let Some(s) = requested_source {
            ensure(s == source, "invalid_request")?;
        }
        if kind == "source-routes" {
            // Historical caller identities cannot acquire new child routes after deletion.
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM source_credentials WHERE source_id=$1)",
            )
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
            ensure(exists, "not_found")?;
        }
        let object = id
            .map(str::to_owned)
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        let previous = if let Some(id) = id {
            Some(
                sqlx::query(&format!(
                    "SELECT t.revision,t.status,to_jsonb(t)-'encrypted_secret'-'delivery_pending' AS snapshot FROM {table} t JOIN source_callers p USING(source_id) WHERE {pk}::text=$1 FOR UPDATE OF t"
                ))
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Fault("not_found"))?,
            )
        } else {
            None
        };
        if method != "DELETE"
            && let Some(old) = &previous
        {
            ensure(
                Some(old.get::<i64, _>("revision")) == v.revision,
                "revision_conflict",
            )?;
        }
        let status = v.status.as_deref().unwrap_or("active");
        ensure(
            matches!(status, "active" | "disabled" | "revoked"),
            "invalid_request",
        )?;
        if method == "DELETE" {
            ensure(kind == "source-credentials", "invalid_request")?;
            // Keep receipt identifiers and audits, but remove live credentials and child routes.
            let routes: Vec<String> = sqlx::query_scalar("SELECT route_id FROM source_routes r WHERE source_id=$1 AND (EXISTS(SELECT 1 FROM source_key_routes k WHERE k.route_id=r.route_id AND k.key_id=$2) OR NOT EXISTS(SELECT 1 FROM source_credentials c WHERE c.source_id=$1 AND c.key_id<>$2))")
                .bind(source).bind(&object).fetch_all(&mut *tx).await?;
            sqlx::query("DELETE FROM source_key_routes WHERE key_id=$1 OR route_id=ANY($2)")
                .bind(&object)
                .bind(&routes)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM source_routes WHERE route_id=ANY($1)")
                .bind(&routes)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM source_request_nonces WHERE key_id=$1")
                .bind(&object)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM source_credentials WHERE key_id=$1")
                .bind(&object)
                .execute(&mut *tx)
                .await?;
        } else if kind == "source-routes" {
            ensure(matches!(status, "active" | "disabled"), "invalid_request")?;
            let retired: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM source_route_tombstones WHERE route_id=$1)",
            )
            .bind(&object)
            .fetch_one(&mut *tx)
            .await?;
            ensure(!retired, "route_retired")?;
            let name = v.route_name.ok_or(Fault("invalid_request"))?;
            ensure(
                !name.trim().is_empty() && name.len() <= 512,
                "invalid_request",
            )?;
            sqlx::query("INSERT INTO source_routes(route_id,source_id,route_name,remark,status) VALUES($1,$2,$3,$4,$5) ON CONFLICT(route_id) DO UPDATE SET route_name=$3,remark=$4,status=$5,revision=source_routes.revision+1,updated_at=now()").bind(&object).bind(source).bind(name).bind(v.remark.unwrap_or_default()).bind(status).execute(&mut *tx).await?;
        } else {
            let unit = v.unit_name.ok_or(Fault("invalid_request"))?;
            ensure(
                !unit.trim().is_empty() && unit.len() <= 512,
                "invalid_request",
            )?;
            if creating_caller {
                sqlx::query("INSERT INTO source_callers(source_id) VALUES($1)")
                    .bind(source)
                    .execute(&mut *tx)
                    .await?;
            }
            let start = v.valid_from.ok_or(Fault("invalid_request"))?;
            let end = v.valid_until;
            ensure(end.is_none_or(|end| end > start), "invalid_request")?;
            if id.is_none() {
                let encoded = crypto::random_secret()?;
                let cipher = crypto::seal(&encoded, &object)?;
                sqlx::query("INSERT INTO source_credentials(key_id,source_id,unit_name,remark,encrypted_secret,encryption_version,status,valid_from,valid_until,created_by) VALUES($1,$2,$3,$4,$5,'v1',$6,$7,$8,$9)").bind(&object).bind(source).bind(unit).bind(v.remark.unwrap_or_default()).bind(cipher).bind(status).bind(start).bind(end).bind(a.id).execute(&mut *tx).await?;
            } else {
                sqlx::query("UPDATE source_credentials SET unit_name=$2,remark=$3,status=$4,valid_from=$5,valid_until=$6,revision=revision+1,updated_at=now() WHERE key_id=$1").bind(&object).bind(unit).bind(v.remark.unwrap_or_default()).bind(status).bind(start).bind(end).execute(&mut *tx).await?;
            }
            if let Some(routes) = v.route_ids {
                ensure(routes.len() <= 1000, "invalid_request")?;
                for route in &routes {
                    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM source_routes WHERE route_id=$1 AND source_id=$2)").bind(route).bind(source).fetch_one(&mut *tx).await?;
                    ensure(valid, "route_unavailable")?;
                }
                sqlx::query("UPDATE source_key_routes SET status='revoked',revision=revision+1,updated_at=now() WHERE key_id=$1").bind(&object).execute(&mut *tx).await?;
                for route in routes {
                    sqlx::query("INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES($1,$2,$3,'active') ON CONFLICT(key_id,route_id) DO UPDATE SET status='active',revision=source_key_routes.revision+1,updated_at=now()").bind(&object).bind(route).bind(source).execute(&mut *tx).await?;
                }
            }
        }
        sqlx::query(
            "UPDATE source_callers SET revision=revision+1,updated_at=now() WHERE source_id=$1",
        )
        .bind(source)
        .execute(&mut *tx)
        .await?;
        let after: Option<Value> = sqlx::query_scalar(&format!("SELECT to_jsonb(t)-'encrypted_secret'-'delivery_pending' FROM {table} t JOIN source_callers p USING(source_id) WHERE {pk}::text=$1")).bind(&object).fetch_optional(&mut *tx).await?;
        audit(&mut tx,source,if kind=="source-credentials"{Some(&object)}else{None},Some(a.id),"source.management.write",&a.request_id,json!({"kind":kind,"id":object,"method":method,"before":previous.as_ref().map(|r|r.get::<Value,_>("snapshot")),"after":after})).await?;
        crate::audit::record(
            &mut tx,
            a,
            Uuid::nil(),
            "source.management.write",
            source,
            json!({"kind":kind,"id":object,"method":method}),
        )
        .await
        .map_err(|error| {
            tracing::error!(request_id=%a.request_id,error=%error,"source management audit failed");
            Fault("temporarily_unavailable")
        })?;
        tx.commit().await?;
        Ok(json!({"id":object}))
    }
    pub async fn associations(
        &self,
        a: &crate::business::Actor,
        key: &str,
        page: i64,
    ) -> Result<Value> {
        ensure(a.platform_admin, "forbidden")?;
        ensure(page > 0 && page <= 100000, "invalid_request")?;
        let rows:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('route_id',r.route_id,'route_name',r.route_name,'remark',r.remark,'status',k.status,'revision',k.revision) FROM source_key_routes k JOIN source_routes r USING(route_id,source_id) WHERE k.key_id=$1 ORDER BY r.route_id LIMIT 31 OFFSET $2").bind(key).bind((page-1)*30).fetch_all(&self.0).await?;
        Ok(json!({"has_more":rows.len()>30,"items":rows.into_iter().take(30).collect::<Vec<_>>()}))
    }
    pub async fn reveal_secret(
        &self,
        a: &crate::business::Actor,
        key: &str,
    ) -> Result<CredentialSecret> {
        ensure(a.platform_admin, "forbidden")?;
        self.management_read_audit(a, "source-credential-secret-reveals")
            .await?;
        let mut tx = self.0.begin().await?;
        let row = sqlx::query(
            "SELECT source_id,encrypted_secret FROM source_credentials WHERE key_id=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(Fault("not_found"))?;
        let cipher: Option<Vec<u8>> = row.get("encrypted_secret");
        let material = crypto::open(&cipher.ok_or(Fault("credential_unavailable"))?, key)?;
        let source: Uuid = row.get("source_id");
        audit(
            &mut tx,
            source,
            Some(key),
            Some(a.id),
            "source.credential.secret_reveal",
            &a.request_id,
            json!({}),
        )
        .await?;
        crate::audit::record(&mut tx, a, Uuid::nil(), "source.credential.secret_reveal", source, json!({"key_id":key}))
            .await.map_err(|error| {
                tracing::error!(request_id=%a.request_id,error=%error,"source secret access audit failed");
                Fault("temporarily_unavailable")
            })?;
        tx.commit().await?;
        Ok(CredentialSecret {
            id: key.into(),
            key_id: key.into(),
            secret: material,
        })
    }
    /// Separate operator command; never called by browser API. One delivery per key.
    pub async fn deliver(&self, key: &str, operator: &str) -> Result<String> {
        ensure(!operator.trim().is_empty(), "invalid_request")?;
        let mut tx = self.0.begin().await?;
        let row=sqlx::query("SELECT source_id,encrypted_secret FROM source_credentials WHERE key_id=$1 AND delivery_pending AND status='active' FOR UPDATE").bind(key).fetch_optional(&mut *tx).await?.ok_or(Fault("credential_unavailable"))?;
        let value = crypto::open(&row.get::<Vec<u8>, _>("encrypted_secret"), key)?;
        sqlx::query("UPDATE source_credentials SET delivery_pending=false WHERE key_id=$1")
            .bind(key)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            row.get("source_id"),
            Some(key),
            None,
            "source.credential.delivered",
            operator,
            json!({}),
        )
        .await?;
        tx.commit().await?;
        Ok(value)
    }
}
impl Repository {
    pub async fn failure(&self, key: &str, request_id: &str, code: &str) -> Result<()> {
        sqlx::query("INSERT INTO source_access_audit(id,source_id,key_id,action,result_code,request_id) SELECT $1,source_id,key_id,'source.request',$3,$4 FROM source_credentials WHERE key_id=$2").bind(Uuid::now_v7()).bind(key).bind(code).bind(request_id).execute(&self.0).await?;
        Ok(())
    }
    pub async fn management_read_audit(
        &self,
        a: &crate::business::Actor,
        kind: &str,
    ) -> Result<()> {
        let mut tx = self.0.begin().await?;
        crate::audit::record(
            &mut tx,
            a,
            Uuid::nil(),
            "source.management.read",
            a.id,
            json!({"kind":kind,"allowed":a.platform_admin}),
        )
        .await
        .map_err(|error| {
            tracing::error!(request_id=%a.request_id,error=%error,"source management audit failed");
            Fault("temporarily_unavailable")
        })?;
        tx.commit().await?;
        Ok(())
    }
}

impl crate::source_access::south::port::MachineStore for Repository {
    async fn accept(&self, signed: Signed, body: Vec<u8>) -> Result<(bool, Value)> {
        self.machine(signed, body).await
    }
}
impl Repository {
    pub async fn set_association(
        &self,
        a: &crate::business::Actor,
        key: &str,
        route: &str,
        status: &str,
        revision: i64,
    ) -> Result<Value> {
        ensure(a.platform_admin, "forbidden")?;
        ensure(matches!(status, "active" | "revoked"), "invalid_request")?;
        let mut tx = self.0.begin().await?;
        let source: Uuid =
            sqlx::query_scalar("SELECT source_id FROM source_credentials WHERE key_id=$1")
                .bind(key)
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(Fault("not_found"))?;
        sqlx::query("SELECT source_id FROM source_callers WHERE source_id=$1 FOR UPDATE")
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
        let credential = sqlx::query(
            "SELECT status,revision FROM source_credentials WHERE key_id=$1 FOR UPDATE",
        )
        .bind(key)
        .fetch_one(&mut *tx)
        .await?;
        ensure(
            credential.get::<i64, _>("revision") == revision,
            "revision_conflict",
        )?;
        let same: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM source_routes WHERE route_id=$1 AND source_id=$2)",
        )
        .bind(route)
        .bind(source)
        .fetch_one(&mut *tx)
        .await?;
        ensure(same, "route_unavailable")?;
        sqlx::query("INSERT INTO source_key_routes(key_id,route_id,source_id,status) VALUES($1,$2,$3,$4) ON CONFLICT(key_id,route_id) DO UPDATE SET status=$4,revision=source_key_routes.revision+1,updated_at=now()").bind(key).bind(route).bind(source).bind(status).execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE source_credentials SET revision=revision+1,updated_at=now() WHERE key_id=$1",
        )
        .bind(key)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE source_callers SET revision=revision+1,updated_at=now() WHERE source_id=$1",
        )
        .bind(source)
        .execute(&mut *tx)
        .await?;
        audit(
            &mut tx,
            source,
            Some(key),
            Some(a.id),
            "source.association.update",
            &a.request_id,
            json!({"route_id":route,"status":status,"credential_revision":revision+1}),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id":route,"key_id":key,"status":status,"credential_revision":revision+1}))
    }
}

pub mod routing;
pub mod routing_worker;
