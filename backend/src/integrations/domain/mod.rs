use sha1::{Digest, Sha1};

/// JS-SDK signs the original URL (including query), excluding its fragment.
pub fn sdk_signature(ticket: &str, nonce: &str, timestamp: i64, url: &str) -> String {
    format!(
        "{:x}",
        Sha1::digest(format!(
            "jsapi_ticket={ticket}&noncestr={nonce}&timestamp={timestamp}&url={url}"
        ))
    )
}

pub fn group_contains_member(group: &serde_json::Value, chat: &str, userid: &str) -> bool {
    !userid.is_empty()
        && group["chat_id"].as_str() == Some(chat)
        && (group["owner"].as_str() == Some(userid)
            || group["member_list"].as_array().is_some_and(|members| {
                members
                    .iter()
                    .any(|member| member["type"] == 1 && member["userid"].as_str() == Some(userid))
            }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_external_contact_and_wrong_group() {
        let group = json!({"chat_id":"chat", "owner":"owner", "member_list":[
            {"type":1,"userid":"employee"}, {"type":2,"userid":"customer"}
        ]});
        assert!(group_contains_member(&group, "chat", "owner"));
        assert!(group_contains_member(&group, "chat", "employee"));
        assert!(!group_contains_member(&group, "other", "employee"));
        assert!(!group_contains_member(&group, "chat", "customer"));
        assert!(!group_contains_member(&json!({}), "chat", ""));
    }

    #[test]
    fn signature_preserves_url_query_order() {
        assert_ne!(
            sdk_signature("ticket", "nonce", 1, "https://site/wecom?a=1&b=2"),
            sdk_signature("ticket", "nonce", 1, "https://site/wecom?b=2&a=1")
        );
        assert_eq!(
            sdk_signature("ticket", "nonce", 1, "https://site/wecom?a=1&b=2"),
            "f3c9d94951ca23d2b358a9f049f5e2a8e8c406a3"
        );
    }
}

pub mod directory;
