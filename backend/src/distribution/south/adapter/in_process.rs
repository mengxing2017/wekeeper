//! Historical forum material adapter over the distribution-owned projection.
//! No forum service or forum database table is accessed.
use crate::{
    business::{Actor, Error, Result, require},
    distribution::{
        domain::content::{Material, SourceReference},
        south::port::content_source::ContentSource,
    },
};
use sqlx::{PgPool, Row};
use uuid::Uuid;
#[derive(Clone)]
pub struct Sources(pub PgPool);
impl ContentSource for Sources {
    async fn check(&self, school: Uuid, references: &[SourceReference]) -> Result<()> {
        for reference in references {
            reference.validate()?;
            require(
                reference.namespace == "builtin.forum"
                    && reference.external_organization_id == school.to_string(),
                "source_entry_not_authorized",
            )?;
            let id = reference
                .external_content_id
                .parse()
                .map_err(|_| Error::NotFound)?;
            let row = self.material(school, id).await?;
            if row.get::<String, _>("version") != reference.version {
                return Err(Error::Policy("source_version_conflict"));
            }
        }
        Ok(())
    }
    async fn read(
        &self,
        actor: &Actor,
        school: Uuid,
        references: &[SourceReference],
    ) -> Result<Vec<Material>> {
        let mut out = Vec::new();
        for reference in references {
            reference.validate()?;
            if reference.namespace == "builtin.forum" {
                require(
                    reference.external_organization_id == school.to_string(),
                    "来源分发工作空间不匹配",
                )?;
                let id = reference
                    .external_content_id
                    .parse()
                    .map_err(|_| Error::NotFound)?;
                actor.authorize(school, "operator")?;
                let material = self.material(school, id).await?;
                if material.get::<String, _>("version") != reference.version {
                    return Err(Error::Policy("source_version_conflict"));
                }
                out.push(Material {
                    reference: reference.clone(),
                    title: material.get("title"),
                    content: material.get("content"),
                    detail_url: format!("/community/{school}/posts/{id}"),
                });
            } else {
                // The browser cannot select another product's identity. Future
                // sources need a separately authenticated adapter/entry point.
                return Err(Error::Policy("source_entry_not_authorized"));
            }
        }
        Ok(out)
    }
}

impl Sources {
    async fn material(&self, school: Uuid, id: Uuid) -> Result<sqlx::postgres::PgRow> {
        sqlx::query("SELECT m.title,m.content,m.version FROM distribution_materials m JOIN distribution_sources s USING(namespace) JOIN distribution_source_organizations o USING(namespace,external_organization_id,school_id) WHERE m.namespace='builtin.forum' AND m.school_id=$1 AND m.external_organization_id=$1::text AND m.external_content_id=$2 AND m.allowed AND s.enabled AND o.enabled")
            .bind(school).bind(id.to_string()).fetch_optional(&self.0).await?.ok_or(Error::Conflict)
    }
}

pub(crate) fn forum_detail_url(school: Uuid, id: Uuid) -> Result<String> {
    let origin = std::env::var("WEKEEPER_FORUM_ORIGIN").unwrap_or_default();
    absolute_detail_url(&origin, &format!("/community/{school}/posts/{id}"))
}

pub(crate) fn absolute_detail_url(origin: &str, path: &str) -> Result<String> {
    let url = reqwest::Url::parse(origin)
        .map_err(|_| Error::Invalid("请配置独立论坛的 WEKEEPER_FORUM_ORIGIN".into()))?;
    require(
        url.scheme() == "https"
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "独立论坛地址必须为 HTTPS origin",
    )?;
    require(
        path.starts_with("/community/") && !path.contains(['?', '#', '\\']),
        "来源详情路径无效",
    )?;
    Ok(format!("{}{path}", origin.trim_end_matches('/')))
}

#[cfg(test)]
mod tests {
    use super::absolute_detail_url;
    #[test]
    fn material_links_use_an_explicit_independent_forum_origin() {
        let path = "/community/school/posts/post";
        assert_eq!(
            absolute_detail_url("https://forum.example/", path).unwrap(),
            "https://forum.example/community/school/posts/post"
        );
        for origin in [
            "",
            "http://forum.example",
            "https://user@forum.example",
            "https://forum.example/path",
            "https://forum.example/?token=x",
            "https://forum.example/#fragment",
        ] {
            assert!(absolute_detail_url(origin, path).is_err(), "{origin}");
        }
    }
}
