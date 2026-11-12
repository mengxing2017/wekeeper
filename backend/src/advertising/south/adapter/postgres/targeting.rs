//! Independent, read-only source-scope discovery.
use crate::advertising::{
    domain::{
        AdvertisingError, Content, Result,
        targeting::{ScopeMatch, ScopeMatchPage, SourceIdentity, TargetScope},
    },
    south::{adapter::postgres::Repository, port::TargetRuleReader},
};
use chrono::{DateTime, Utc};
use sqlx::Row;
use uuid::Uuid;

impl TargetRuleReader for Repository {
    async fn matching_page(
        &self,
        identity: SourceIdentity,
        at: DateTime<Utc>,
        after: Option<Uuid>,
    ) -> Result<ScopeMatchPage> {
        let rows = sqlx::query(
            "SELECT r.id,r.revision,r.scope_kind,r.platform_id,r.unit_id,\
                    m.id AS material_id,v.id AS version_id,v.name,v.body,v.link \
             FROM ad_placement_rules r \
             JOIN ad_materials m ON m.id=r.material_id \
             JOIN ad_material_versions v ON v.material_id=m.id AND v.id=m.current_version_id \
             WHERE EXISTS(SELECT 1 FROM business_units u WHERE u.platform_id=$1 AND u.id=$2) \
               AND r.enabled AND NOT m.archived AND r.valid_from <= $3 \
               AND (r.valid_until IS NULL OR $3 < r.valid_until) \
               AND ($4::uuid IS NULL OR r.id > $4) \
               AND (r.scope_kind='global' OR (r.scope_kind='platform' AND r.platform_id=$1) \
                    OR (r.scope_kind='unit' AND r.platform_id=$1 AND r.unit_id=$2)) \
             ORDER BY r.id LIMIT 31",
        )
        .bind(identity.platform_id)
        .bind(identity.unit_id)
        .bind(at)
        .bind(after)
        .fetch_all(&self.0)
        .await
        .map_err(|error| {
            tracing::error!(error = %error, "advertising scope discovery failed");
            AdvertisingError("temporarily_unavailable")
        })?;
        let has_more = rows.len() > 30;
        let mut items = Vec::with_capacity(rows.len().min(30));
        for row in rows.into_iter().take(30) {
            let platform: Option<Uuid> = row.get("platform_id");
            let unit: Option<Uuid> = row.get("unit_id");
            let scope = match (row.get::<String, _>("scope_kind").as_str(), platform, unit) {
                ("global", None, None) => TargetScope::Global {},
                ("platform", Some(platform_id), None) => TargetScope::Platform { platform_id },
                ("unit", Some(platform_id), Some(unit_id)) => TargetScope::Unit {
                    platform_id,
                    unit_id,
                },
                _ => return Err(AdvertisingError("temporarily_unavailable")),
            };
            items.push(ScopeMatch {
                rule_id: row.get("id"),
                rule_revision: row.get("revision"),
                scope,
                material_id: row.get("material_id"),
                material_version_id: row.get("version_id"),
                content: Content {
                    name: row.get("name"),
                    body: row.get("body"),
                    link: row.get("link"),
                },
            });
        }
        let next_after = if has_more {
            items.last().map(|item| item.rule_id)
        } else {
            None
        };
        Ok(ScopeMatchPage { items, next_after })
    }
}
