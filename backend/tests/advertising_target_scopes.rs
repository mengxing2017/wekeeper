//! Read-only advertising scope discovery against an explicitly isolated database.
use chrono::{Duration, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::advertising::{
    domain::targeting::{SourceIdentity, TargetScope},
    north::local::MatchingRules,
    south::adapter::postgres::Repository,
};

#[allow(clippy::too_many_arguments)]
async fn rule(
    pool: &PgPool,
    actor: Uuid,
    material: Uuid,
    kind: &str,
    platform: Option<Uuid>,
    unit: Option<Uuid>,
    enabled: bool,
    start: chrono::DateTime<Utc>,
    end: Option<chrono::DateTime<Utc>>,
) -> Uuid {
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO ad_placement_rules(id,name,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,valid_until,created_by,updated_by) VALUES($1,'rule',$2,$3,$4,$5,$6,$7,$8,$9,$9)")
        .bind(id).bind(material).bind(kind).bind(platform).bind(unit).bind(enabled).bind(start).bind(end).bind(actor).execute(pool).await.unwrap();
    id
}

#[tokio::test]
#[ignore = "requires explicit isolated WEKEEPER_TEST_DATABASE_URL"]
async fn matches_business_scopes_without_routes_or_selection() {
    let url = std::env::var("WEKEEPER_TEST_DATABASE_URL").expect("isolated database required");
    let base = PgPool::connect(&url).await.unwrap();
    let schema = format!("ad_scope_{}", Uuid::new_v4().simple());
    sqlx::query(&format!("CREATE SCHEMA {schema}"))
        .execute(&base)
        .await
        .unwrap();
    let search = format!("SET search_path TO {schema},public");
    let pool = PgPoolOptions::new()
        .max_connections(4)
        .after_connect(move |connection, _| {
            let sql = search.clone();
            Box::pin(async move {
                sqlx::query(&sql).execute(connection).await?;
                Ok(())
            })
        })
        .connect(&url)
        .await
        .unwrap();
    wekeeper_api::database::migrate(&pool).await.unwrap();
    let actor = Uuid::new_v4();
    let platform = Uuid::new_v4();
    let other_platform = Uuid::new_v4();
    let unit = Uuid::new_v4();
    let sibling = Uuid::new_v4();
    let other_unit = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'admin',true)",
    )
    .bind(actor)
    .bind(actor.to_string())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO source_callers(source_id) VALUES($1),($2)")
        .bind(platform)
        .bind(other_platform)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO business_units(id,platform_id,name,created_by,updated_by) VALUES($1,$4,'unit',$6,$6),($2,$4,'sibling',$6,$6),($3,$5,'other',$6,$6)")
        .bind(unit).bind(sibling).bind(other_unit).bind(platform).bind(other_platform).bind(actor).execute(&pool).await.unwrap();
    use axum::{
        body::{Body, to_bytes},
        http::Request,
    };
    use tower::ServiceExt;
    let other_actor = Uuid::new_v4();
    sqlx::query("INSERT INTO accounts(id,username,display_name,platform_admin) VALUES($1,$2,'operator',false)")
        .bind(other_actor).bind(other_actor.to_string()).execute(&pool).await.unwrap();
    let app = wekeeper_api::bootstrap::build_application(pool.clone());
    let (token, session) = app
        .identity
        .0
        .session(actor, "browser", false)
        .await
        .unwrap();
    let (other_token, other_session) = app
        .identity
        .0
        .session(other_actor, "browser", false)
        .await
        .unwrap();
    let router = wekeeper_api::http::app_router(app);
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../api-contract/fixtures/scoped-ad-material.json"
    ))
    .unwrap();
    let create_body =
        serde_json::json!({"data":{"type":"ad-materials","attributes":fixture["create"]}});
    let post = |path: &str, token: &str, csrf: &str, body: &serde_json::Value| {
        Request::builder()
            .method("POST")
            .uri(path)
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "http://localhost:5173")
            .header("x-csrf-token", csrf)
            .header("content-type", "application/vnd.api+json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    let denied = router
        .clone()
        .oneshot(post(
            "/api/v2/admin/ad-materials",
            &other_token,
            &other_session.csrf,
            &create_body,
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    let response = router
        .clone()
        .oneshot(post(
            "/api/v2/admin/ad-materials",
            &token,
            &session.csrf,
            &create_body,
        ))
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let created: serde_json::Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap()).unwrap();
    let created_id = Uuid::parse_str(created["data"]["id"].as_str().unwrap()).unwrap();
    assert_eq!(created["data"]["attributes"]["revision"], 1);
    assert_eq!(
        created["data"]["attributes"]["content"],
        fixture["create"]["content"]
    );
    let old_version: Uuid = created["data"]["attributes"]["content_version"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let publish_path = format!("/api/v2/admin/ad-materials/{created_id}/publications");
    let publish_body = serde_json::json!({"data":{"type":"ad-material-publications","attributes":fixture["publish"]}});
    let (first, second) = tokio::join!(
        router
            .clone()
            .oneshot(post(&publish_path, &token, &session.csrf, &publish_body)),
        router
            .clone()
            .oneshot(post(&publish_path, &token, &session.csrf, &publish_body))
    );
    let (first, second) = (
        first.unwrap().status().as_u16(),
        second.unwrap().status().as_u16(),
    );
    assert!((first == 200 && second == 409) || (first == 409 && second == 200));
    let old_body: String = sqlx::query_scalar("SELECT body FROM ad_material_versions WHERE id=$1")
        .bind(old_version)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old_body, "广告正文");
    let get = Request::builder()
        .uri(format!("/api/v2/admin/ad-materials/{created_id}"))
        .header("cookie", format!("wekeeper_session={token}"))
        .body(Body::empty())
        .unwrap();
    let current = router.clone().oneshot(get).await.unwrap();
    assert_eq!(current.status(), 200);
    let current: serde_json::Value =
        serde_json::from_slice(&to_bytes(current.into_body(), 100_000).await.unwrap()).unwrap();
    assert_eq!(current["data"]["attributes"]["revision"], 2);
    assert_eq!(
        current["data"]["attributes"]["content"],
        fixture["publish"]["content"]
    );
    let audited: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE object_id=$1 AND action IN ('advertising.material.create','advertising.material.publish') AND actor_id=$2")
        .bind(created_id).bind(actor).fetch_one(&pool).await.unwrap();
    assert_eq!(audited, 2);
    let rule_path = "/api/v2/admin/ad-placement-rules";
    let mut new_rule = fixture["rule_create"].clone();
    new_rule["material_id"] = serde_json::json!(created_id);
    new_rule["valid_from"] = serde_json::json!((Utc::now() - Duration::hours(1)).to_rfc3339());
    let mut created_rules = Vec::new();
    for scope in [
        serde_json::json!({"kind":"global"}),
        serde_json::json!({"kind":"platform","platform_id":platform}),
        serde_json::json!({"kind":"unit","platform_id":platform,"unit_id":unit}),
    ] {
        new_rule["scope"] = scope.clone();
        let body = serde_json::json!({"data":{"type":"ad-placement-rules","attributes":new_rule}});
        let denied = router
            .clone()
            .oneshot(post(rule_path, &other_token, &other_session.csrf, &body))
            .await
            .unwrap();
        assert_eq!(denied.status(), 403);
        let response = router
            .clone()
            .oneshot(post(rule_path, &token, &session.csrf, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let response: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap())
                .unwrap();
        assert_eq!(response["data"]["attributes"]["scope"], scope);
        created_rules.push(Uuid::parse_str(response["data"]["id"].as_str().unwrap()).unwrap());
    }
    for (path, expected_id) in [
        ("/api/v2/admin/ad-materials", created_id),
        (rule_path, created_rules[0]),
    ] {
        for (session_token, expected_status) in [(&token, 200), (&other_token, 403)] {
            let request = Request::builder()
                .uri(format!("{path}?page=1"))
                .header("cookie", format!("wekeeper_session={session_token}"))
                .body(Body::empty())
                .unwrap();
            let response = router.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), expected_status);
            if expected_status == 200 {
                let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap();
                let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                assert!(
                    body["data"]["attributes"]["items"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|item| item["id"] == expected_id.to_string())
                );
                assert_eq!(body["data"]["attributes"]["has_more"], false);
            }
        }
        let invalid = Request::builder()
            .uri(format!("{path}?page=0"))
            .header("cookie", format!("wekeeper_session={token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(router.clone().oneshot(invalid).await.unwrap().status(), 400);
    }
    new_rule["scope"] =
        serde_json::json!({"kind":"unit","platform_id":platform,"unit_id":other_unit});
    let invalid_body =
        serde_json::json!({"data":{"type":"ad-placement-rules","attributes":new_rule}});
    let invalid_scope = router
        .clone()
        .oneshot(post(rule_path, &token, &session.csrf, &invalid_body))
        .await
        .unwrap();
    assert_eq!(invalid_scope.status(), 400);
    let read_rule = Request::builder()
        .uri(format!("{rule_path}/{}", created_rules[0]))
        .header("cookie", format!("wekeeper_session={token}"))
        .body(Body::empty())
        .unwrap();
    let read_rule = router.clone().oneshot(read_rule).await.unwrap();
    assert_eq!(read_rule.status(), 200);
    let discovered = MatchingRules(Repository(pool.clone()))
        .page(
            SourceIdentity {
                platform_id: platform,
                unit_id: unit,
            },
            Utc::now(),
            None,
        )
        .await
        .unwrap();
    let discovered_ids: std::collections::HashSet<_> =
        discovered.items.iter().map(|item| item.rule_id).collect();
    assert!(created_rules.iter().all(|id| discovered_ids.contains(id)));
    let rule_audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE object_id=ANY($1) AND action='advertising.placement.create' AND actor_id=$2")
        .bind(&created_rules).bind(actor).fetch_one(&pool).await.unwrap();
    assert_eq!(rule_audit, 3);
    let mut archive = fixture["archive"].clone();
    archive["expected_revision"] = serde_json::json!(2);
    let archive_body =
        serde_json::json!({"data":{"type":"ad-material-archives","attributes":archive}});
    let archive_request = |token: &str, csrf: &str, body: &serde_json::Value| {
        Request::builder()
            .method("PUT")
            .uri(format!("/api/v2/admin/ad-materials/{created_id}/archive"))
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "http://localhost:5173")
            .header("x-csrf-token", csrf)
            .header("content-type", "application/vnd.api+json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    assert_eq!(
        router
            .clone()
            .oneshot(archive_request(
                &other_token,
                &other_session.csrf,
                &archive_body
            ))
            .await
            .unwrap()
            .status(),
        403
    );
    let stale_archive =
        serde_json::json!({"data":{"type":"ad-material-archives","attributes":fixture["archive"]}});
    assert_eq!(
        router
            .clone()
            .oneshot(archive_request(&token, &session.csrf, &stale_archive))
            .await
            .unwrap()
            .status(),
        409
    );
    let archived = router
        .clone()
        .oneshot(archive_request(&token, &session.csrf, &archive_body))
        .await
        .unwrap();
    assert_eq!(archived.status(), 200);
    let archived: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(archived.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(archived["data"]["attributes"]["archived"], true);
    assert_eq!(archived["data"]["attributes"]["revision"], 3);
    let after_archive = MatchingRules(Repository(pool.clone()))
        .page(
            SourceIdentity {
                platform_id: platform,
                unit_id: unit,
            },
            Utc::now(),
            None,
        )
        .await
        .unwrap();
    assert!(
        after_archive
            .items
            .iter()
            .all(|item| !created_rules.contains(&item.rule_id))
    );
    let versions_uri = format!("/api/v2/admin/ad-materials/{created_id}/versions?page=1");
    for (session_token, status) in [(&token, 200), (&other_token, 403)] {
        let req = Request::builder()
            .uri(&versions_uri)
            .header("cookie", format!("wekeeper_session={session_token}"))
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), status);
        if status == 200 {
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX)
                    .await
                    .unwrap(),
            )
            .unwrap();
            let versions = body["data"]["attributes"]["items"].as_array().unwrap();
            assert_eq!(versions.len(), 2);
            assert_eq!(versions[0]["version"], 1);
            assert_eq!(versions[0]["content"], fixture["create"]["content"]);
            assert_eq!(versions[1]["version"], 2);
            assert_eq!(versions[1]["content"], fixture["publish"]["content"]);
            assert_eq!(versions[1]["created_by"], actor.to_string());
            assert_eq!(body["data"]["attributes"]["has_more"], false);
        }
    }
    for (path, status) in [
        (
            format!("/api/v2/admin/ad-materials/{created_id}/versions?page=0"),
            400,
        ),
        (
            format!("/api/v2/admin/ad-materials/{}/versions", Uuid::new_v4()),
            404,
        ),
    ] {
        let req = Request::builder()
            .uri(path)
            .header("cookie", format!("wekeeper_session={token}"))
            .body(Body::empty())
            .unwrap();
        assert_eq!(router.clone().oneshot(req).await.unwrap().status(), status);
    }
    let archive_audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE object_id=$1 AND action='advertising.material.archive' AND actor_id=$2")
        .bind(created_id).bind(actor).fetch_one(&pool).await.unwrap();
    assert_eq!(archive_audit, 1);
    assert_eq!(
        router
            .clone()
            .oneshot(post(&publish_path, &token, &session.csrf, &publish_body))
            .await
            .unwrap()
            .status(),
        409
    );
    let state_body = serde_json::json!({"data":{"type":"ad-placement-rule-states","attributes":fixture["rule_state"]}});
    let put_state = |rule: Uuid, token: &str, csrf: &str| {
        Request::builder()
            .method("PUT")
            .uri(format!("{rule_path}/{rule}/state"))
            .header("cookie", format!("wekeeper_session={token}"))
            .header("origin", "http://localhost:5173")
            .header("x-csrf-token", csrf)
            .header("content-type", "application/vnd.api+json")
            .body(Body::from(state_body.to_string()))
            .unwrap()
    };
    let denied = router
        .clone()
        .oneshot(put_state(
            created_rules[0],
            &other_token,
            &other_session.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    for rule in &created_rules {
        let response = router
            .clone()
            .oneshot(put_state(*rule, &token, &session.csrf))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
    let stale = router
        .clone()
        .oneshot(put_state(created_rules[0], &token, &session.csrf))
        .await
        .unwrap();
    assert_eq!(stale.status(), 409);
    let disabled = MatchingRules(Repository(pool.clone()))
        .page(
            SourceIdentity {
                platform_id: platform,
                unit_id: unit,
            },
            Utc::now(),
            None,
        )
        .await
        .unwrap();
    assert!(
        disabled
            .items
            .iter()
            .all(|item| !created_rules.contains(&item.rule_id))
    );
    let state_audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE object_id=ANY($1) AND action='advertising.placement.state' AND actor_id=$2")
        .bind(&created_rules).bind(actor).fetch_one(&pool).await.unwrap();
    assert_eq!(state_audit, 3);
    // No source routes exist; material and rules use only business identities.
    let material = Uuid::new_v4();
    let version = Uuid::new_v4();
    sqlx::query("INSERT INTO ad_materials(id,created_by) VALUES($1,$2)")
        .bind(material)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ad_material_versions(id,material_id,version,name,body,created_by) VALUES($1,$2,1,'素材','正文',$3)").bind(version).bind(material).bind(actor).execute(&pool).await.unwrap();
    sqlx::query("UPDATE ad_materials SET current_version_id=$2 WHERE id=$1")
        .bind(material)
        .bind(version)
        .execute(&pool)
        .await
        .unwrap();
    let at = Utc::now();
    let start = at - Duration::hours(1);
    let global = rule(
        &pool, actor, material, "global", None, None, true, start, None,
    )
    .await;
    let platform_rule = rule(
        &pool,
        actor,
        material,
        "platform",
        Some(platform),
        None,
        true,
        start,
        None,
    )
    .await;
    let unit_rule = rule(
        &pool,
        actor,
        material,
        "unit",
        Some(platform),
        Some(unit),
        true,
        start,
        None,
    )
    .await;
    let other_rule = rule(
        &pool,
        actor,
        material,
        "unit",
        Some(other_platform),
        Some(other_unit),
        true,
        start,
        None,
    )
    .await;
    rule(
        &pool, actor, material, "global", None, None, false, start, None,
    )
    .await;
    rule(
        &pool,
        actor,
        material,
        "global",
        None,
        None,
        true,
        start,
        Some(at),
    )
    .await;
    rule(
        &pool,
        actor,
        material,
        "global",
        None,
        None,
        true,
        at + Duration::hours(1),
        None,
    )
    .await;
    let unpublished = Uuid::new_v4();
    sqlx::query("INSERT INTO ad_materials(id,created_by) VALUES($1,$2)")
        .bind(unpublished)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    rule(
        &pool,
        actor,
        unpublished,
        "global",
        None,
        None,
        true,
        start,
        None,
    )
    .await;
    let wrong_version = sqlx::query("UPDATE ad_materials SET current_version_id=$2 WHERE id=$1")
        .bind(unpublished)
        .bind(version)
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        wrong_version.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let archived = Uuid::new_v4();
    let archived_version = Uuid::new_v4();
    sqlx::query("INSERT INTO ad_materials(id,created_by,archived) VALUES($1,$2,true)")
        .bind(archived)
        .bind(actor)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ad_material_versions(id,material_id,version,name,body,created_by) VALUES($1,$2,1,'archived','body',$3)")
        .bind(archived_version).bind(archived).bind(actor).execute(&pool).await.unwrap();
    sqlx::query("UPDATE ad_materials SET current_version_id=$2 WHERE id=$1")
        .bind(archived)
        .bind(archived_version)
        .execute(&pool)
        .await
        .unwrap();
    rule(
        &pool, actor, archived, "global", None, None, true, start, None,
    )
    .await;
    let invalid = sqlx::query("INSERT INTO ad_placement_rules(id,name,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,created_by,updated_by) VALUES($1,'rule',$2,'unit',$3,$4,true,$5,$6,$6)")
        .bind(Uuid::new_v4()).bind(material).bind(platform).bind(other_unit).bind(start).bind(actor).execute(&pool).await.unwrap_err();
    assert_eq!(
        invalid.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let legacy = sqlx::query("INSERT INTO ad_placement_rules(id,name,material_id,scope_kind,enabled,valid_from,created_by,updated_by) VALUES($1,'rule',$2,'route',true,$3,$4,$4)")
        .bind(Uuid::new_v4()).bind(material).bind(start).bind(actor).execute(&pool).await.unwrap_err();
    assert_eq!(
        legacy.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let matcher = MatchingRules(Repository(pool.clone()));
    let identity = SourceIdentity {
        platform_id: platform,
        unit_id: unit,
    };
    let page = matcher.page(identity, at, None).await.unwrap();
    let ids: std::collections::HashSet<_> = page.items.iter().map(|x| x.rule_id).collect();
    assert_eq!(
        ids,
        [global, platform_rule, unit_rule].into_iter().collect()
    );
    assert!(page.items.iter().any(|item| item.scope
        == TargetScope::Unit {
            platform_id: platform,
            unit_id: unit
        }
        && item.material_version_id == version
        && item.content.body == "正文"));
    assert!(page.next_after.is_none());
    let sibling_page = matcher
        .page(
            SourceIdentity {
                platform_id: platform,
                unit_id: sibling,
            },
            at,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        sibling_page
            .items
            .iter()
            .map(|x| x.rule_id)
            .collect::<std::collections::HashSet<_>>(),
        [global, platform_rule].into_iter().collect()
    );
    let other_page = matcher
        .page(
            SourceIdentity {
                platform_id: other_platform,
                unit_id: other_unit,
            },
            at,
            None,
        )
        .await
        .unwrap();
    assert_eq!(
        other_page
            .items
            .iter()
            .map(|x| x.rule_id)
            .collect::<std::collections::HashSet<_>>(),
        [global, other_rule].into_iter().collect()
    );
    assert!(
        matcher
            .page(
                SourceIdentity {
                    platform_id: other_platform,
                    unit_id: unit
                },
                at,
                None
            )
            .await
            .unwrap()
            .items
            .is_empty()
    );
    let changed = sqlx::query("UPDATE ad_material_versions SET body='modified' WHERE id=$1")
        .bind(version)
        .execute(&pool)
        .await;
    assert!(changed.is_err());
    // Multiple rules can match; pagination does not impose delivery precedence.
    for _ in 0..31 {
        rule(
            &pool, actor, material, "global", None, None, true, start, None,
        )
        .await;
    }
    let first = matcher.page(identity, at, None).await.unwrap();
    assert_eq!(first.items.len(), 30);
    let second = matcher.page(identity, at, first.next_after).await.unwrap();
    assert_eq!(second.items.len(), 4);
    assert!(second.next_after.is_none());
    let first_ids: std::collections::HashSet<_> = first.items.iter().map(|x| x.rule_id).collect();
    assert!(second.items.iter().all(|x| !first_ids.contains(&x.rule_id)));
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&base)
        .await
        .unwrap();
    base.close().await;
}
