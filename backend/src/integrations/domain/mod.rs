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
}

pub mod assets;
pub mod directory;
