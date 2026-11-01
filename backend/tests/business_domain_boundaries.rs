use std::{fs, path::Path};
fn visit(path: &Path, files: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, files)
        } else if path.extension().is_some_and(|x| x == "rs") {
            files.push(path)
        }
    }
}
#[test]
fn distribution_only_the_in_process_adapter_may_depend_on_community() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/distribution");
    let mut files = Vec::new();
    visit(&root, &mut files);
    for file in files {
        let source = fs::read_to_string(&file).unwrap();
        if !file.ends_with("adapter/in_process.rs") {
            assert!(
                !source.contains("crate::community"),
                "forum type dependency: {}",
                file.display()
            );
        }
        let normalized = source.to_lowercase();
        for table in ["posts", "comments", "cases", "school_resources"] {
            for keyword in ["from", "join", "update", "into"] {
                assert!(
                    !normalized.contains(&format!("{keyword} {table} ")),
                    "forum SQL in distribution: {}",
                    file.display()
                );
            }
        }
    }
}
#[test]
fn official_ui_does_not_import_mobile_account_components() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../frontend/apps/web/src/features/operations/wecom-senders");
    for file in ["index.tsx", "policies.tsx", "api/index.ts"] {
        let source = fs::read_to_string(root.join(file)).unwrap();
        for forbidden in [
            "mobile-delivery",
            "SchoolExecutionResources",
            "DeviceAccountBinding",
            "sending_accounts",
        ] {
            assert!(!source.contains(forbidden));
        }
    }
}
#[test]
fn browser_materials_cannot_impersonate_builtin_source_or_confirm() {
    use wekeeper_api::distribution::domain::broadcast::DraftInput;
    let group = uuid::Uuid::new_v4();
    let mut value = serde_json::json!({"content_source":"materials","group_ids":[group],"materials":[{"namespace":"builtin.forum","external_organization_id":"1","external_content_id":"1","version":"v1"}]});
    assert!(
        serde_json::from_value::<DraftInput>(value.clone())
            .unwrap()
            .validate()
            .is_err()
    );
    value["materials"][0]["namespace"] = serde_json::json!("test.notice");
    serde_json::from_value::<DraftInput>(value.clone())
        .unwrap()
        .validate()
        .unwrap();
    value["confirmed"] = serde_json::json!(true);
    assert!(serde_json::from_value::<DraftInput>(value).is_err());
}
#[tokio::test]
async fn new_domain_errors_do_not_suggest_phone_authorization_for_official_work() {
    use axum::response::IntoResponse;
    for (code, expected_status) in [
        ("preview_expired", 409),
        ("source_content_unavailable", 409),
        ("source_entry_not_authorized", 403),
    ] {
        let response = wekeeper_api::business::Error::Policy(code).into_response();
        assert_eq!(response.status().as_u16(), expected_status);
        let bytes = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["errors"][0]["code"], code);
        assert!(
            !value["errors"][0]["detail"]
                .as_str()
                .unwrap()
                .contains("手机执行约束")
        );
    }
}
