use crate::{
    bootstrap::Application,
    business::{Actor, Error, Grant, Page, Result, text, uuid},
    community::domain::{CommentInput, PostInput},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use uuid::Uuid;
fn get<'a>(v: &'a HashMap<String, String>, k: &str) -> &'a str {
    v.get(k).map(String::as_str).unwrap_or("")
}
fn page(v: &HashMap<String, String>) -> Page {
    Page {
        page: Some(get(v, "page").parse().unwrap_or(1)),
        page_size: Some(get(v, "limit").parse().unwrap_or(20)),
        search: Some(get(v, "keywords").into()),
        ..Default::default()
    }
}
async fn profile(a: &Application, u: &Actor, token: &str) -> Result<Value> {
    let id = a.legacy.alias("fa_user", u.id).await?;
    let school = if let Some(s) = u.bound_school_id {
        a.legacy.alias("fa_school", s).await?
    } else {
        0
    };
    let mut user = json!({"id":id,"nickname":u.display_name,"username":u.username,"avatar":"","school_id":school,"school_locked":if school>0{1}else{0}});
    if let Some(s) = u.bound_school_id
        && let Some(school) = a
            .schools
            .list(u)
            .await?
            .into_iter()
            .find(|v| v["id"] == json!(s))
    {
        let mut info = school["config"].clone();
        if !info.is_object() {
            info = json!({});
        }
        info["name"] = school["name"].clone();
        info["id"] = user["school_id"].clone();
        user["school_info"] = info;
    }
    let mut auth = user.clone();
    auth["token"] = json!(token);
    auth["createtime"] = json!(chrono::Utc::now().timestamp());
    auth["expires_in"] = json!(28800);
    Ok(json!({"user":user,"userinfo":user,"auth":auth}))
}
async fn post(a: &Application, u: &Actor, s: Uuid, mut p: Value) -> Result<Value> {
    let id = uuid(&p, "id")?;
    let author = uuid(&p, "author_id")?;
    let mut images = Vec::new();
    if let Some(list) = p["images"].as_array() {
        for path in list {
            images.push(
                a.legacy
                    .signed_media(s, path.as_str().unwrap_or(""), &a.options.public_origin)
                    .await?,
            );
        }
    }
    let likes = a.community.reactions(u, s).await?;
    let following = a.community.follows(u, s).await?;
    let liked = likes
        .iter()
        .any(|r| r["post_id"].as_str() == Some(id.to_string().as_str()) && r["kind"] == "like");
    let followed = following.iter().any(|r| r["id"] == json!(author));
    p["id"] = json!(a.legacy.alias("fa_discover", id).await?);
    p["user_id"] = json!(a.legacy.alias("fa_user", author).await?);
    p["school_id"] = json!(a.legacy.alias("fa_school", s).await?);
    p["nickname"] = p["author_name"].clone();
    p["avatar"] = json!("");
    p["text"] = p["content"].clone();
    p["description"] = p["content"].clone();
    p["coverimages_"] = json!(images);
    p["coverimages"] = json!(images.join(","));
    p["image_urlLists"] = p["coverimages"].clone();
    p["image_url"] = json!(images.first().cloned().unwrap_or_default());
    p["coverimage"] = p["image_url"].clone();
    p["coverPicVideos"] = json!(
        images
            .iter()
            .map(|url| json!({"type":"image","url":url}))
            .collect::<Vec<_>>()
    );
    p["type"] = json!("image");
    p["favorNum"] = p.get("like_count").cloned().unwrap_or(json!(0));
    p["commentNum"] = p.get("comment_count").cloned().unwrap_or(json!(0));
    p["isFavor"] = json!(if liked { 1 } else { 0 });
    p["isfavor"] = p["isFavor"].clone();
    p["isAttention"] = json!(if followed { 1 } else { 0 });
    p["isattention"] = p["isAttention"].clone();
    p["canFollowAuthor"] = json!(u.id != author);
    p["can_follow_author"] = json!(if u.id != author { 1 } else { 0 });
    p["createtime"] = json!(
        p["created_at"]
            .as_str()
            .and_then(|x| chrono::DateTime::parse_from_rfc3339(x).ok())
            .map(|x| x.timestamp())
            .unwrap_or(0)
    );
    Ok(p)
}
pub async fn dispatch(
    a: &Application,
    controller: &str,
    method: &str,
    headers: &axum::http::HeaderMap,
    v: HashMap<String, String>,
) -> Result<Value> {
    if !["Common", "User"].contains(&controller) {
        return Err(Error::NotFound);
    }
    let token = headers
        .get("token")
        .and_then(|h| h.to_str().ok())
        .unwrap_or("");
    if method == "legalDocs" {
        let resources = a.schools.0.resources(Uuid::nil(), false).await?;
        let mut docs = json!({});
        for r in resources.iter().filter(|r| r["kind"] == "legal") {
            let title = r["title"].as_str().unwrap_or("");
            let key = if title.contains("隐私") || title == "privacy" {
                "privacy"
            } else if title.contains("协议") || title == "agreement" {
                "agreement"
            } else {
                "notice"
            };
            let content = r["content"].as_str().unwrap_or("");
            docs[key] = serde_json::from_str(content)
                .unwrap_or_else(|_| json!({"title":title,"intro":content,"sections":[]}));
        }
        return Ok(json!({"docs":docs,"contact":{}}));
    }
    if method == "third" {
        let code = text(get(&v, "code"), 1, 256)?;
        let id = a.integrations.miniapp(&code).await?;
        let (token, u) = a.identity.0.session(id, "miniapp", false).await?;
        return profile(a, &u, &token).await;
    }
    if method == "login" {
        let name = if get(&v, "account").is_empty() {
            get(&v, "username")
        } else {
            get(&v, "account")
        };
        let (t, u) = a
            .identity
            .login(crate::identity::north::local::Login {
                username: name.into(),
                password: get(&v, "password").into(),
            })
            .await?;
        a.identity.0.logout(&crate::business::hash(&t)).await?;
        let (t, u) = a.identity.0.session(u.id, "miniapp", false).await?;
        return profile(a, &u, &t).await;
    }
    if method == "schoolList" {
        let list = a.schools.directory().await?;
        let mut rows = Vec::new();
        for mut r in list {
            if !get(&v, "keyword").is_empty()
                && !r["name"]
                    .as_str()
                    .unwrap_or("")
                    .contains(get(&v, "keyword"))
            {
                continue;
            }
            r["id"] = json!(a.legacy.alias("fa_school", uuid(&r, "id")?).await?);
            rows.push(r);
        }
        return Ok(json!({"list":rows}));
    }
    let u = if token.is_empty() {
        let s = a.legacy.resolve("fa_school", get(&v, "school_id")).await?;
        if ![
            "index",
            "detail",
            "type",
            "homeBanner",
            "legalDocs",
            "customerService",
            "groupQrcode",
            "showCommentLists",
        ]
        .contains(&method)
            || !a.legacy.public(s).await?
        {
            return Err(Error::Unauthorized);
        }
        Actor {
            id: Uuid::nil(),
            username: "guest".into(),
            display_name: "访客".into(),
            platform_admin: false,
            bound_school_id: Some(s),
            grants: vec![Grant {
                school_id: s,
                role: "member".into(),
            }],
            csrf: String::new(),
            session_hash: String::new(),
            request_id: String::new(),
            browser: false,
            wecom_authenticated: false,
        }
    } else {
        a.identity.0.authenticate(token, "miniapp").await?
    };
    match method {
        "refreshUser" | "index" if controller == "User" => return profile(a, &u, token).await,
        "logout" => {
            a.identity.0.logout(&u.session_hash).await?;
            return Ok(json!({}));
        }
        "profile" => {
            a.identity
                .0
                .profile(u.id, &text(get(&v, "nickname"), 1, 100)?)
                .await?;
            return Ok(json!({}));
        }
        "bindSchool" => {
            let s = a.legacy.resolve("fa_school", get(&v, "school_id")).await?;
            a.schools.bind(&u, s).await?;
            return profile(
                a,
                &a.identity.0.authenticate(token, "miniapp").await?,
                token,
            )
            .await;
        }
        _ => {}
    }
    let s = u
        .bound_school_id
        .or_else(|| u.grants.first().map(|g| g.school_id))
        .ok_or_else(|| Error::Invalid("请先绑定学校".into()))?;
    u.authorize(s, "member")?;
    if !get(&v, "school_id").is_empty() && get(&v, "school_id") != "0" {
        let requested = a.legacy.resolve("fa_school", get(&v, "school_id")).await?;
        if requested != s {
            return Err(Error::Forbidden);
        }
    }
    let postid = if !get(&v, "discover_id").is_empty() {
        get(&v, "discover_id")
    } else {
        get(&v, "id")
    };
    match method {
        "index" | "doMyDiscoverLists" | "attentionData" => {
            let mut p = page(&v);
            p.mine = Some(method == "doMyDiscoverLists");
            let (page_num, size) = (p.page.unwrap_or(1), p.page_size.unwrap_or(20));
            let records = a.community.posts(&u, s, p).await?;
            let follows = a.community.follows(&u, s).await?;
            let mut rows = Vec::new();
            for r in records {
                if method == "attentionData" && !follows.iter().any(|f| f["id"] == r["author_id"]) {
                    continue;
                }
                rows.push(post(a, &u, s, r).await?);
            }
            let mut counts = a.legacy.counts(&u, s).await?;
            counts["data"] = json!({"data":rows,"current_page":page_num,"last_page":page_num+if rows.len() as i64==size{1}else{0},"per_page":size});
            if method == "doMyDiscoverLists" {
                let mut grouped = std::collections::BTreeMap::<String, Vec<Value>>::new();
                for row in rows {
                    let key = row["created_at"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(10)
                        .collect();
                    grouped.entry(key).or_default().push(row);
                }
                let list = grouped
                    .into_iter()
                    .rev()
                    .map(|(day, list)| json!({"monthDate":day,"istoday":0,"list":list}))
                    .collect::<Vec<_>>();
                return Ok(
                    json!({"list":list,"count":counts["discoverCount"],"total":(counts["discoverCount"].as_i64().unwrap_or(0)+size-1)/size}),
                );
            }
            if method != "index" {
                return Ok(
                    json!({"list":rows,"count":rows.len(),"total":page_num+if rows.len() as i64==size{1}else{0}}),
                );
            }
            Ok(counts)
        }
        "detail" => {
            let id = a.legacy.resolve("fa_discover", postid).await?;
            post(a, &u, s, a.community.detail(&u, s, id).await?).await
        }
        "publish" => {
            let images = get(&v, "coverimages")
                .split(',')
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect();
            a.community
                .post(
                    &u,
                    s,
                    None,
                    PostInput {
                        title: get(&v, "title").into(),
                        content: get(&v, "content").into(),
                        images,
                        tags: get(&v, "tag_ids")
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned)
                            .collect(),
                        revision: None,
                    },
                )
                .await?;
            Ok(json!({}))
        }
        "delData" => {
            let id = a.legacy.resolve("fa_discover", postid).await?;
            let p = a.community.detail(&u, s, id).await?;
            a.community
                .visibility(
                    &u,
                    s,
                    id,
                    json!({"visibility":"deleted","top_sort":0,"revision":p["revision"]}),
                )
                .await?;
            Ok(json!({}))
        }
        "dolike" | "doLike" | "isFavor" => {
            if get(&v, "type") == "2" {
                let id = a
                    .legacy
                    .resolve("fa_discover_comment", get(&v, "comment_id"))
                    .await?;
                return a.legacy.comment_like(&u, s, id).await;
            }
            let id = a.legacy.resolve("fa_discover", postid).await?;
            let reactions = a.community.reactions(&u, s).await?;
            let liked = reactions
                .iter()
                .any(|r| r["post_id"] == json!(id) && r["kind"] == "like");
            if method == "isFavor" {
                return Ok(json!({"isFavor":if liked{1}else{0}}));
            }
            a.community.react(&u, s, id, "like", !liked).await?;
            Ok(json!({"isFavor":if liked{0}else{1}}))
        }
        "doComment" => {
            let id = a.legacy.resolve("fa_discover", postid).await?;
            let parent = if get(&v, "type") == "2" {
                Some(
                    a.legacy
                        .resolve("fa_discover_comment", get(&v, "comment_id"))
                        .await?,
                )
            } else {
                None
            };
            a.community
                .comment(
                    &u,
                    s,
                    id,
                    CommentInput {
                        content: get(&v, "content").into(),
                        parent_id: parent,
                    },
                )
                .await?;
            Ok(json!({}))
        }
        "showCommentLists" | "doCommentSub" | "doCommentLists" => {
            let id = a.legacy.resolve("fa_discover", postid).await?;
            let comments = a.community.comments(&u, s, Some(id), page(&v)).await?;
            let mut rows = Vec::new();
            for mut c in comments {
                let cid = uuid(&c, "id")?;
                c["id"] = json!(a.legacy.alias("fa_discover_comment", cid).await?);
                c["discover_id"] = json!(a.legacy.alias("fa_discover", id).await?);
                c["user_id"] = json!(a.legacy.alias("fa_user", uuid(&c, "author_id")?).await?);
                c["nickname"] = c["author_name"].clone();
                c["avatar"] = json!("");
                c["replyLists"] = json!([]);
                rows.push(c);
            }
            Ok(json!({"list":rows,"total":1,"count":rows.len()}))
        }
        "doAttention" => {
            let id = a.legacy.resolve("fa_user", get(&v, "attention_id")).await?;
            let list = a.community.follows(&u, s).await?;
            a.community
                .follow(&u, s, id, !list.iter().any(|r| r["id"] == json!(id)))
                .await?;
            Ok(json!({}))
        }
        "doAttentionLists" | "doLikeLists" => {
            let mut list = if method == "doLikeLists" {
                a.legacy.received_likes(s, u.id).await?
            } else if get(&v, "type") == "2" {
                a.legacy.fans(s, u.id).await?
            } else {
                a.community.follows(&u, s).await?
            };
            for r in &mut list {
                r["id"] = json!(a.legacy.alias("fa_user", uuid(r, "id")?).await?);
                r["nickname"] = r["display_name"].clone();
                r["avatar"] = json!("");
            }
            Ok(json!({"list":list,"count":list.len(),"total":1}))
        }
        "userDataLists" => {
            let c = a.legacy.counts(&u, s).await?;
            let likes = a.legacy.received_likes(s, u.id).await?;
            let following = a.community.follows(&u, s).await?;
            let fans = a.legacy.fans(s, u.id).await?;
            Ok(
                json!({"favorLists":likes,"attentionLists":following,"beAttentionLists":fans,"favorListsCount":c["favorCount"],"attentionListsCount":c["attentionCount"],"beAttentionListsCount":c["beAttentionCount"],"favorTotalCount":a.community.reactions(&u,s).await?.iter().filter(|r|r["kind"]=="like").count()}),
            )
        }
        "showMessageLists" => {
            let mut rows = a.community.notifications(&u, s).await?;
            for r in &mut rows {
                r["id"] = json!(a.legacy.alias("fa_discover_log", uuid(r, "id")?).await?);
                r["readdata"] = json!(if r["read_at"].is_null() { 0 } else { 1 });
            }
            Ok(json!({"list":rows,"count":1}))
        }
        "doMessageRead" => {
            for id in get(&v, "ids").split(',').filter(|v| !v.is_empty()) {
                a.community
                    .read(&u, s, a.legacy.resolve("fa_discover_log", id).await?)
                    .await?;
            }
            Ok(json!({}))
        }
        "report" | "feedback" => {
            let id = if method == "report" {
                Some(a.legacy.resolve("fa_discover", postid).await?)
            } else {
                None
            };
            a.community.case(&u,s,json!({"kind":if method=="report"{"report"}else{"feedback"},"post_id":id,"content":if method=="report"{get(&v,"reason")}else{get(&v,"content")},"contact":get(&v,"contact")})).await?;
            Ok(json!({}))
        }
        "type" | "homeBanner" | "groupQrcode" | "customerService" | "legalDocs" => {
            let kind = match method {
                "type" => "tag",
                "homeBanner" => "banner",
                "groupQrcode" => "qrcode",
                "customerService" => "service",
                _ => "legal",
            };
            let mut rows = a
                .schools
                .resources(&u, s)
                .await?
                .into_iter()
                .filter(|r| r["kind"] == kind)
                .collect::<Vec<_>>();
            for r in &mut rows {
                r["id"] = json!(a.legacy.alias("fa_school_resource", uuid(r, "id")?).await?);
                r["name"] = r["title"].clone();
                r["image"] = r["url"].clone();
                r["qrcode"] = r["url"].clone();
                r["note"] = r["content"].clone();
                r["enabled"] = json!(1);
            }
            if method == "customerService" {
                let mut qrcodes = json!({"business":"","join":"","service":""});
                let mut fallback = String::new();
                let mut note = String::new();
                let mut enabled = 1;
                for r in &rows {
                    let title = r["title"].as_str().unwrap_or("");
                    let url = r["url"].as_str().unwrap_or("");
                    if title.ends_with("enabled") && r["content"] == "0" {
                        enabled = 0;
                    }
                    if !url.is_empty() {
                        fallback = url.into();
                        for key in ["business", "join", "service"] {
                            if title.contains(key) {
                                qrcodes[key] = json!(url);
                            }
                        }
                    } else if !title.ends_with("enabled") {
                        note = r["content"].as_str().unwrap_or("").into();
                    }
                }
                for key in ["business", "join", "service"] {
                    if qrcodes[key] == "" {
                        qrcodes[key] = json!(fallback);
                    }
                }
                return Ok(
                    json!({"config":{"enabled":enabled,"qrcode":fallback,"qrcodes":qrcodes,"note":note}}),
                );
            }
            if ["groupQrcode", "customerService"].contains(&method) {
                Ok(json!({"config":rows.first()}))
            } else if method == "type" {
                Ok(json!(rows))
            } else {
                Ok(json!({"list":rows}))
            }
        }
        "homeBannerClick" => {
            a.schools
                .click(
                    &u,
                    s,
                    a.legacy
                        .resolve("fa_school_resource", get(&v, "id"))
                        .await?,
                )
                .await?;
            Ok(json!({}))
        }
        "locationData" => {
            let schools = a.schools.list(&u).await?;
            Ok(json!({"list":schools}))
        }
        _ => Err(Error::Invalid(
            "该旧接口已停用，请使用微信登录或联系管理员处理账号".into(),
        )),
    }
}
