//! Read-only advertising scope discovery against an explicitly isolated database.
use chrono::{Duration, Utc};
use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;
use wekeeper_api::advertising::{
    domain::targeting::{SourceIdentity, TargetScope},
    north::local::MatchingAds,
    south::adapter::postgres::Repository,
};

async fn published_material(pool: &PgPool, actor: Uuid) -> Uuid {
    let material = Uuid::new_v4();
    let version = Uuid::new_v4();
    sqlx::query("INSERT INTO ad_materials(id,created_by) VALUES($1,$2)")
        .bind(material)
        .bind(actor)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO ad_material_versions(id,material_id,version,name,body,created_by) VALUES($1,$2,1,'广告','正文',$3)")
        .bind(version).bind(material).bind(actor).execute(pool).await.unwrap();
    sqlx::query("UPDATE ad_materials SET current_version_id=$2 WHERE id=$1")
        .bind(material)
        .bind(version)
        .execute(pool)
        .await
        .unwrap();
    material
}

#[allow(clippy::too_many_arguments)]
async fn placement(
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
    sqlx::query("INSERT INTO ad_placements(id,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,valid_until,created_by,updated_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$9)")
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
    let placement_path = "/api/v2/admin/ad-placements";
    let mut new_placement = fixture["placement_create"].clone();
    new_placement["material_id"] = serde_json::json!(created_id);
    new_placement["valid_from"] = serde_json::json!((Utc::now() - Duration::hours(1)).to_rfc3339());
    let mut created_placements = Vec::new();
    for scope in [
        serde_json::json!({"kind":"global"}),
        serde_json::json!({"kind":"platform","platform_id":platform}),
        serde_json::json!({"kind":"unit","platform_id":platform,"unit_id":unit}),
    ] {
        new_placement["scope"] = scope.clone();
        let body = serde_json::json!({"data":{"type":"ad-placements","attributes":new_placement}});
        let denied = router
            .clone()
            .oneshot(post(
                placement_path,
                &other_token,
                &other_session.csrf,
                &body,
            ))
            .await
            .unwrap();
        assert_eq!(denied.status(), 403);
        let response = router
            .clone()
            .oneshot(post(placement_path, &token, &session.csrf, &body))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let response: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 100_000).await.unwrap())
                .unwrap();
        assert_eq!(response["data"]["attributes"]["scope"], scope);
        assert!(response["data"]["attributes"].get("name").is_none());
        created_placements.push(Uuid::parse_str(response["data"]["id"].as_str().unwrap()).unwrap());
        // A direct material/scope binding cannot be created a second time,
        // including scopes with NULL platform/unit components.
        let duplicate = router
            .clone()
            .oneshot(post(placement_path, &token, &session.csrf, &body))
            .await
            .unwrap();
        assert_eq!(duplicate.status(), 409);
    }
    let old_endpoint = Request::builder()
        .uri("/api/v2/admin/ad-placement-rules")
        .header("cookie", format!("wekeeper_session={token}"))
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        router.clone().oneshot(old_endpoint).await.unwrap().status(),
        404
    );
    let mut named_binding = new_placement.clone();
    named_binding["name"] = serde_json::json!("不再存在的规则名称");
    let named_binding =
        serde_json::json!({"data":{"type":"ad-placements","attributes":named_binding}});
    assert_eq!(
        router
            .clone()
            .oneshot(post(placement_path, &token, &session.csrf, &named_binding))
            .await
            .unwrap()
            .status(),
        400
    );
    for (path, expected_id) in [
        ("/api/v2/admin/ad-materials", created_id),
        (placement_path, created_placements[0]),
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
    new_placement["scope"] =
        serde_json::json!({"kind":"unit","platform_id":platform,"unit_id":other_unit});
    let invalid_body =
        serde_json::json!({"data":{"type":"ad-placements","attributes":new_placement}});
    let invalid_scope = router
        .clone()
        .oneshot(post(placement_path, &token, &session.csrf, &invalid_body))
        .await
        .unwrap();
    assert_eq!(invalid_scope.status(), 400);
    let read_placement = Request::builder()
        .uri(format!("{placement_path}/{}", created_placements[0]))
        .header("cookie", format!("wekeeper_session={token}"))
        .body(Body::empty())
        .unwrap();
    let read_placement = router.clone().oneshot(read_placement).await.unwrap();
    assert_eq!(read_placement.status(), 200);
    let discovered = MatchingAds(Repository(pool.clone()))
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
    let discovered_ids: std::collections::HashSet<_> = discovered
        .items
        .iter()
        .map(|item| item.placement_id)
        .collect();
    assert!(
        created_placements
            .iter()
            .all(|id| discovered_ids.contains(id))
    );
    let placement_audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE object_id=ANY($1) AND action='advertising.placement.create' AND actor_id=$2")
        .bind(&created_placements).bind(actor).fetch_one(&pool).await.unwrap();
    assert_eq!(placement_audit, 3);
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
    let after_archive = MatchingAds(Repository(pool.clone()))
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
            .all(|item| !created_placements.contains(&item.placement_id))
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
    let state_body = serde_json::json!({"data":{"type":"ad-placement-states","attributes":fixture["placement_state"]}});
    let put_state = |placement: Uuid, token: &str, csrf: &str| {
        Request::builder()
            .method("PUT")
            .uri(format!("{placement_path}/{placement}/state"))
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
            created_placements[0],
            &other_token,
            &other_session.csrf,
        ))
        .await
        .unwrap();
    assert_eq!(denied.status(), 403);
    for placement in &created_placements {
        let response = router
            .clone()
            .oneshot(put_state(*placement, &token, &session.csrf))
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
    }
    let stale = router
        .clone()
        .oneshot(put_state(created_placements[0], &token, &session.csrf))
        .await
        .unwrap();
    assert_eq!(stale.status(), 409);
    let disabled = MatchingAds(Repository(pool.clone()))
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
            .all(|item| !created_placements.contains(&item.placement_id))
    );
    let state_audit: i64 = sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE object_id=ANY($1) AND action='advertising.placement.state' AND actor_id=$2")
        .bind(&created_placements).bind(actor).fetch_one(&pool).await.unwrap();
    assert_eq!(state_audit, 3);
    // No source routes exist; material and placements use only business identities.
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
    let global = placement(
        &pool, actor, material, "global", None, None, true, start, None,
    )
    .await;
    let platform_placement = placement(
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
    let unit_placement = placement(
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
    let other_placement = placement(
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
    placement(
        &pool,
        actor,
        published_material(&pool, actor).await,
        "global",
        None,
        None,
        false,
        start,
        None,
    )
    .await;
    placement(
        &pool,
        actor,
        published_material(&pool, actor).await,
        "global",
        None,
        None,
        true,
        start,
        Some(at),
    )
    .await;
    placement(
        &pool,
        actor,
        published_material(&pool, actor).await,
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
    placement(
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
    placement(
        &pool, actor, archived, "global", None, None, true, start, None,
    )
    .await;
    let invalid = sqlx::query("INSERT INTO ad_placements(id,material_id,scope_kind,platform_id,unit_id,enabled,valid_from,created_by,updated_by) VALUES($1,$2,'unit',$3,$4,true,$5,$6,$6)")
        .bind(Uuid::new_v4()).bind(material).bind(platform).bind(other_unit).bind(start).bind(actor).execute(&pool).await.unwrap_err();
    assert_eq!(
        invalid.as_database_error().unwrap().code().as_deref(),
        Some("23503")
    );
    let legacy = sqlx::query("INSERT INTO ad_placements(id,material_id,scope_kind,enabled,valid_from,created_by,updated_by) VALUES($1,$2,'route',true,$3,$4,$4)")
        .bind(Uuid::new_v4()).bind(material).bind(start).bind(actor).execute(&pool).await.unwrap_err();
    assert_eq!(
        legacy.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let matcher = MatchingAds(Repository(pool.clone()));
    let identity = SourceIdentity {
        platform_id: platform,
        unit_id: unit,
    };
    let page = matcher.page(identity, at, None).await.unwrap();
    let ids: std::collections::HashSet<_> = page.items.iter().map(|x| x.placement_id).collect();
    assert_eq!(
        ids,
        [global, platform_placement, unit_placement]
            .into_iter()
            .collect()
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
            .map(|x| x.placement_id)
            .collect::<std::collections::HashSet<_>>(),
        [global, platform_placement].into_iter().collect()
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
            .map(|x| x.placement_id)
            .collect::<std::collections::HashSet<_>>(),
        [global, other_placement].into_iter().collect()
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
    // Multiple advertisements can match; pagination does not impose delivery precedence.
    for _ in 0..31 {
        placement(
            &pool,
            actor,
            published_material(&pool, actor).await,
            "global",
            None,
            None,
            true,
            start,
            None,
        )
        .await;
    }
    let first = matcher.page(identity, at, None).await.unwrap();
    assert_eq!(first.items.len(), 30);
    let second = matcher.page(identity, at, first.next_after).await.unwrap();
    assert_eq!(second.items.len(), 4);
    assert!(second.next_after.is_none());
    let first_ids: std::collections::HashSet<_> =
        first.items.iter().map(|x| x.placement_id).collect();
    assert!(
        second
            .items
            .iter()
            .all(|x| !first_ids.contains(&x.placement_id))
    );

    // Force the preferred unit placement onto the last page of discovery.
    let last_unit_placement = Uuid::from_u128(u128::MAX);
    sqlx::query("UPDATE ad_placements SET id=$2 WHERE id=$1")
        .bind(unit_placement)
        .bind(last_unit_placement)
        .execute(&pool)
        .await
        .unwrap();
    let preferred = matcher.preferred_candidates(identity, at).await.unwrap();
    assert_eq!(preferred.len(), 1);
    assert_eq!(preferred[0].placement_id, last_unit_placement);

    // Multiple advertisements may share a scope; discovery is not multi-ad composition.
    let tied_unit_placement = placement(
        &pool,
        actor,
        published_material(&pool, actor).await,
        "unit",
        Some(platform),
        Some(unit),
        true,
        start,
        None,
    )
    .await;
    let preferred = matcher.preferred_candidates(identity, at).await.unwrap();
    assert_eq!(
        preferred
            .iter()
            .map(|item| item.placement_id)
            .collect::<std::collections::HashSet<_>>(),
        [last_unit_placement, tied_unit_placement]
            .into_iter()
            .collect()
    );
    let preferred_sibling = matcher
        .preferred_candidates(
            SourceIdentity {
                platform_id: platform,
                unit_id: sibling,
            },
            at,
        )
        .await
        .unwrap();
    assert_eq!(preferred_sibling.len(), 1);
    assert_eq!(preferred_sibling[0].placement_id, platform_placement);
    assert!(
        matcher
            .preferred_candidates(
                SourceIdentity {
                    platform_id: other_platform,
                    unit_id: unit,
                },
                at
            )
            .await
            .unwrap()
            .is_empty()
    );

    // Disabling all higher scopes leaves only the valid global candidates.
    sqlx::query("UPDATE ad_placements SET enabled=false WHERE scope_kind != 'global'")
        .execute(&pool)
        .await
        .unwrap();
    let preferred_global = matcher.preferred_candidates(identity, at).await.unwrap();
    assert_eq!(preferred_global.len(), 32);
    assert!(
        preferred_global
            .iter()
            .all(|item| item.scope == TargetScope::Global {})
    );
    sqlx::query("UPDATE ad_placements SET enabled=false")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        matcher
            .preferred_candidates(identity, at)
            .await
            .unwrap()
            .is_empty()
    );
    // HTTP validation counts Han characters rather than their UTF-8 bytes.
    for (length, status) in [(140, 200), (141, 400)] {
        let mut attributes = fixture["create"].clone();
        attributes["content"]["body"] = serde_json::json!("汉".repeat(length));
        let body = serde_json::json!({"data":{"type":"ad-materials","attributes":attributes}});
        let req = post("/api/v2/admin/ad-materials", &token, &session.csrf, &body);
        let response = router.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), status);
        if status == 400 {
            let body: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap())
                    .unwrap();
            assert_eq!(body["errors"][0]["code"], "invalid_ad_content");
        }
    }
    // Immediate decisions use business-unit counters, never route/workspace counters.
    use wekeeper_api::advertising::{
        domain::decision::DecisionInput, north::local::AdDecisionService,
    };
    let service = AdDecisionService(Repository(pool.clone()));
    let input = DecisionInput {
        source: identity,
        message_id: Uuid::new_v4(),
        original: "即时消息".into(),
        channel: "mobile".into(),
    };
    let none = service.decide(input.clone()).await.unwrap();
    assert!(none.selected_ad.is_none());
    assert_eq!(none.content, input.original);
    assert_eq!(none.selection_count, 0);
    sqlx::query("UPDATE ad_placements SET enabled=true WHERE id=ANY($1)")
        .bind(vec![
            last_unit_placement,
            tied_unit_placement,
            platform_placement,
        ])
        .execute(&pool)
        .await
        .unwrap();
    // Replaying a no-ad message does not retroactively insert advertising.
    assert_eq!(service.decide(input.clone()).await.unwrap(), none);
    let mut input = DecisionInput {
        message_id: Uuid::new_v4(),
        ..input
    };
    let first_message_id = input.message_id;
    let first = service.decide(input.clone()).await.unwrap();
    assert_eq!(first.selection_count, 1);
    let selected = first.selected_ad.as_ref().unwrap();
    assert!([last_unit_placement, tied_unit_placement].contains(&selected.placement_id));
    let mut jobs = tokio::task::JoinSet::new();
    for _ in 0..8 {
        let input = input.clone();
        let pool = pool.clone();
        jobs.spawn(async move {
            AdDecisionService(Repository(pool))
                .decide(input)
                .await
                .unwrap()
        });
    }
    while let Some(result) = jobs.join_next().await {
        assert_eq!(result.unwrap(), first);
    }
    let mut conflicting = input.clone();
    conflicting.original.push('变');
    assert_eq!(
        service.decide(conflicting).await.unwrap_err().0,
        "message_conflict"
    );
    input.message_id = Uuid::new_v4();
    let second = service.decide(input.clone()).await.unwrap();
    assert_eq!(second.selection_count, 2);
    assert_ne!(
        second.selected_ad.as_ref().unwrap().placement_id,
        selected.placement_id
    );
    let sibling_input = DecisionInput {
        source: SourceIdentity {
            platform_id: platform,
            unit_id: sibling,
        },
        message_id: Uuid::new_v4(),
        ..input.clone()
    };
    let sibling_first = service.decide(sibling_input).await.unwrap();
    assert_eq!(sibling_first.selection_count, 1);
    assert_eq!(
        sibling_first.selected_ad.unwrap().placement_id,
        platform_placement
    );
    // Channel overflow preserves the complete original and does not consume a selection.
    let overflow = service
        .decide(DecisionInput {
            original: "x".repeat(4000),
            channel: "wecom".into(),
            message_id: Uuid::new_v4(),
            ..input.clone()
        })
        .await
        .unwrap();
    assert_eq!(overflow.content, "x".repeat(4000));
    assert_eq!(overflow.selection_count, 2);
    assert!(overflow.selected_ad.is_none());
    for _ in 0..8 {
        let input = DecisionInput {
            message_id: Uuid::new_v4(),
            ..input.clone()
        };
        let pool = pool.clone();
        jobs.spawn(async move {
            AdDecisionService(Repository(pool))
                .decide(input)
                .await
                .unwrap()
        });
    }
    let mut counts = Vec::new();
    while let Some(result) = jobs.join_next().await {
        counts.push(result.unwrap().selection_count);
    }
    counts.sort();
    assert_eq!(counts, (3..=10).collect::<Vec<_>>());
    sqlx::query("UPDATE ad_placements SET enabled=false")
        .execute(&pool)
        .await
        .unwrap();
    // Decisions are immutable message facts; changing availability never recalls them.
    input.message_id = Uuid::new_v4();
    let unavailable = service.decide(input.clone()).await.unwrap();
    assert_eq!(unavailable.content, input.original);
    assert_eq!(unavailable.selection_count, 10);
    assert!(unavailable.selected_ad.is_none());
    assert_eq!(
        service
            .decide(DecisionInput {
                message_id: first_message_id,
                ..input
            })
            .await
            .unwrap(),
        first
    );
    pool.close().await;
    sqlx::query(&format!("DROP SCHEMA {schema} CASCADE"))
        .execute(&base)
        .await
        .unwrap();
    base.close().await;
}
