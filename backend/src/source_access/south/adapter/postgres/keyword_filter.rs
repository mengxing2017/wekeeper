use super::*;
use crate::source_access::{domain::keyword_filter as k, south::port::KeywordStore};
type Tx<'a> = Transaction<'a, Postgres>;
#[derive(Clone)]
pub struct KeywordRepository(pub PgPool);
async fn guard(tx: &mut Tx<'_>) -> Result<()> {
    crate::distribution::south::adapter::postgres::routing::source_guard(tx)
        .await
        .map_err(|_| Fault("temporarily_unavailable"))
}
pub(crate) async fn context_for_scope(
    tx: &mut Tx<'_>,
    source: Uuid,
    route: String,
    unit: Option<Uuid>,
) -> Result<(k::Context, bool, Vec<k::Rule>)> {
    let ids = vec!["global".to_owned()];
    let rows=sqlx::query("SELECT h.id,h.revision,h.version_id,v.entries,v.filter_enabled FROM keyword_scope_heads h JOIN keyword_rule_versions v ON v.id=h.version_id WHERE h.id=ANY($1) ORDER BY h.id FOR SHARE OF h").bind(&ids).fetch_all(&mut **tx).await?;
    ensure(rows.len() == ids.len(), "keyword_rules_unavailable")?;
    let mut filter_enabled = false;
    let mut versions = vec![];
    let mut rules = vec![];
    for row in rows {
        let scope: String = row.get("id");
        let version: Uuid = row.get("version_id");
        versions.push(k::ScopeVersion {
            scope_id: scope.clone(),
            revision: row.get("revision"),
            version_id: version,
        });
        filter_enabled = row.get("filter_enabled");
        if !filter_enabled {
            continue;
        }
        let entries: Vec<k::Entry> = serde_json::from_value(row.get("entries"))?;
        // Legacy imports retain every nonempty restriction, with deterministic normalized deduplication.
        let mut seen = std::collections::BTreeSet::new();
        for entry in entries {
            if entry.enabled {
                let normalized = k::normalize(&entry.word);
                ensure(!normalized.trim().is_empty(), "invalid_keyword")?;
                if seen.insert(normalized.clone()) {
                    rules.push(k::Rule {
                        id: digest(format!("{scope}\n{normalized}").as_bytes()),
                        scope_id: scope.clone(),
                        version_id: version,
                        word: entry.word,
                    });
                }
            }
        }
    }
    ensure(rules.len() <= 10000, "keyword_limit_exceeded")?;
    Ok((
        k::Context {
            source_id: source,
            route_id: route,
            unit_id: unit,
            channel: None,
            versions,
        },
        filter_enabled,
        rules,
    ))
}
async fn scope_access(tx: &mut Tx<'_>, scope: &str) -> Result<()> {
    let row = sqlx::query("SELECT * FROM keyword_scope_heads WHERE id=$1")
        .bind(scope)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(Fault("not_found"))?;
    ensure(row.get::<String, _>("id") == "global", "forbidden")?;
    Ok(())
}
impl KeywordStore for KeywordRepository {
    async fn execute(&self, actor: &crate::business::Actor, command: k::Command) -> Result<Value> {
        let mut tx = self.0.begin().await?;
        if matches!(&command, k::Command::Publish { .. }) {
            guard(&mut tx).await?;
        }
        super::routing::administrator(&mut tx, actor.id).await?;
        let mut audit_context = match &command {
            k::Command::Scopes => json!({"action":"list_scopes"}),
            k::Command::Versions { scope, .. } => json!({"action":"versions","scope_id":scope}),
            k::Command::Publish { scope, .. } => json!({"action":"publish","scope_id":scope}),
        };
        let value = match command {
            k::Command::Scopes => {
                let rows = sqlx::query(
                    "SELECT h.* FROM keyword_scope_heads h WHERE h.id = 'global' ORDER BY h.id",
                )
                .fetch_all(&mut *tx)
                .await?;
                let mut items = vec![];
                for r in rows {
                    let id: String = r.get("id");
                    match scope_access(&mut tx,&id).await{Ok(())=>items.push(json!({"id":id,"kind":r.get::<String,_>("kind"),"object_id":r.get::<String,_>("object_id"),"channel":r.get::<Option<String>,_>("channel"),"label":r.get::<String,_>("label"),"revision":r.get::<i64,_>("revision"),"version_id":r.get::<Uuid,_>("version_id"),"can_edit":true})),Err(Fault("forbidden"))=>{},Err(e)=>return Err(e)}
                }
                json!({"items":items})
            }
            k::Command::Versions { scope, page } => {
                scope_access(&mut tx, &scope).await?;
                let rows:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(v) FROM keyword_rule_versions v WHERE scope_id=$1 ORDER BY revision DESC LIMIT 31 OFFSET $2").bind(&scope).bind((page-1)*30).fetch_all(&mut *tx).await?;
                let items: Vec<Value> = rows
                    .iter()
                    .take(30)
                    .map(|v| {
                        let mut v = v.clone();
                        if let Some(entries) = v["entries"].as_array_mut() {
                            for entry in entries {
                                entry["normalized"] =
                                    json!(k::normalize(entry["word"].as_str().unwrap_or("")));
                            }
                        }
                        v
                    })
                    .collect();
                json!({"items":items,"has_more":rows.len()>30,"algorithm":k::ALGORITHM,"unicode_version":k::UNICODE_VERSION})
            }
            k::Command::Publish { scope, input } => {
                scope_access(&mut tx, &scope).await?;
                k::validate(&input.entries)?;
                let previous = sqlx::query(
                    "SELECT h.revision,v.filter_enabled FROM keyword_scope_heads h JOIN keyword_rule_versions v ON v.id=h.version_id WHERE h.id=$1 FOR UPDATE OF h",
                )
                .bind(&scope)
                .fetch_one(&mut *tx)
                .await?;
                let revision: i64 = previous.get("revision");
                ensure(revision == input.expected_revision, "revision_conflict")?;
                let version = Uuid::now_v7();
                sqlx::query("INSERT INTO keyword_rule_versions(id,scope_id,revision,entries,actor_id,reason,filter_enabled) VALUES($1,$2,$3,$4,$5,$6,$7)").bind(version).bind(&scope).bind(revision+1).bind(json!(input.entries)).bind(actor.id).bind(input.reason.trim()).bind(input.filter_enabled).execute(&mut *tx).await?;
                sqlx::query(
                    "UPDATE keyword_scope_heads SET revision=revision+1,version_id=$2 WHERE id=$1",
                )
                .bind(&scope)
                .bind(version)
                .execute(&mut *tx)
                .await?;

                audit_context["previous_filter_enabled"] =
                    json!(previous.get::<bool, _>("filter_enabled"));
                audit_context["filter_enabled"] = json!(input.filter_enabled);
                audit_context["previous_revision"] = json!(revision);
                audit_context["revision"] = json!(revision + 1);
                json!({"id":scope,"revision":revision+1,"version_id":version})
            }
        };
        crate::audit::record(
            &mut tx,
            actor,
            "source.keyword.access",
            actor.id,
            audit_context,
        )
        .await
        .map_err(|_| Fault("temporarily_unavailable"))?;
        tx.commit().await?;
        Ok(value)
    }
}
