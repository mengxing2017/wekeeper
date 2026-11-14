use super::Repository;
use crate::advertising::{
    domain::{
        AdvertisingError, Content, Result,
        decision::{Decision, DecisionInput, SelectedAd, select_one},
    },
    south::port::AdDecisionStore,
};
use sqlx::Row;

fn unavailable(error: impl std::fmt::Display) -> AdvertisingError {
    tracing::error!(error = %error, "immediate advertisement decision failed");
    AdvertisingError("temporarily_unavailable")
}

impl AdDecisionStore for Repository {
    async fn decide(&self, input: DecisionInput) -> Result<Decision> {
        let mut tx = self.0.begin().await.map_err(unavailable)?;
        sqlx::query("INSERT INTO ad_unit_counters(platform_id,unit_id) VALUES($1,$2) ON CONFLICT DO NOTHING")
            .bind(input.source.platform_id).bind(input.source.unit_id).execute(&mut *tx).await
            .map_err(|error| {
                if error.as_database_error().is_some_and(|db| db.code().as_deref()==Some("23503")) {
                    AdvertisingError("invalid_ad_scope")
                } else {unavailable(error)}
            })?;
        // A unit has one counter even when it uses platform/global advertising.
        let count:i64 = sqlx::query_scalar("SELECT selected_count FROM ad_unit_counters WHERE platform_id=$1 AND unit_id=$2 FOR UPDATE")
            .bind(input.source.platform_id).bind(input.source.unit_id).fetch_one(&mut *tx).await.map_err(unavailable)?;
        let existing = sqlx::query("SELECT original,channel,decision FROM ad_message_decisions WHERE platform_id=$1 AND unit_id=$2 AND message_id=$3")
            .bind(input.source.platform_id).bind(input.source.unit_id).bind(input.message_id)
            .fetch_optional(&mut *tx).await.map_err(unavailable)?;
        if let Some(row) = existing {
            if row.get::<String, _>("original") != input.original
                || row.get::<String, _>("channel") != input.channel
            {
                return Err(AdvertisingError("message_conflict"));
            }
            let decision = serde_json::from_value(row.get("decision")).map_err(unavailable)?;
            tx.commit().await.map_err(unavailable)?;
            return Ok(decision);
        }
        let rows = sqlx::query(
            "WITH matching AS (SELECT p.id,p.material_id,v.name,v.body,v.link, \
                CASE p.scope_kind WHEN 'unit' THEN 2 WHEN 'platform' THEN 1 ELSE 0 END AS priority \
             FROM ad_placements p JOIN ad_materials m ON m.id=p.material_id \
             JOIN ad_material_versions v ON v.id=m.current_version_id AND v.material_id=m.id \
             WHERE p.enabled AND NOT m.archived AND p.valid_from<=clock_timestamp() \
               AND (p.valid_until IS NULL OR clock_timestamp()<p.valid_until) \
               AND (p.scope_kind='global' OR (p.scope_kind='platform' AND p.platform_id=$1) \
                    OR (p.scope_kind='unit' AND p.platform_id=$1 AND p.unit_id=$2))) \
             SELECT id,material_id,name,body,link FROM matching WHERE priority=(SELECT max(priority) FROM matching) ORDER BY id"
        ).bind(input.source.platform_id).bind(input.source.unit_id).fetch_all(&mut *tx).await.map_err(unavailable)?;
        let candidates = rows
            .into_iter()
            .map(|row| SelectedAd {
                placement_id: row.get("id"),
                material_id: row.get("material_id"),
                content: Content {
                    name: row.get("name"),
                    body: row.get("body"),
                    link: row.get("link"),
                },
            })
            .collect();
        let decision = select_one(&input, count, candidates);
        sqlx::query("INSERT INTO ad_message_decisions(platform_id,unit_id,message_id,original,channel,decision) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(input.source.platform_id).bind(input.source.unit_id).bind(input.message_id)
            .bind(&input.original).bind(&input.channel).bind(serde_json::to_value(&decision).map_err(unavailable)?)
            .execute(&mut *tx).await.map_err(unavailable)?;
        if decision.selected_ad.is_some() {
            sqlx::query(
                "UPDATE ad_unit_counters SET selected_count=$3 WHERE platform_id=$1 AND unit_id=$2",
            )
            .bind(input.source.platform_id)
            .bind(input.source.unit_id)
            .bind(decision.selection_count)
            .execute(&mut *tx)
            .await
            .map_err(unavailable)?;
        }
        tx.commit().await.map_err(unavailable)?;
        Ok(decision)
    }
}
