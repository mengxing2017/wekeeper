use crate::business::{Result, require, text};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ImageRef {
    Asset { id: Uuid },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ResourceAction {
    None {},
    InternalPage {
        page: String,
        #[serde(default)]
        object_id: Option<Uuid>,
    },
    ExternalLink {
        url: String,
    },
    Phone {
        number: String,
    },
    CopyText {
        text: String,
    },
}
impl Default for ResourceAction {
    fn default() -> Self {
        Self::None {}
    }
}
pub fn https(url: &str) -> bool {
    !url.chars()
        .any(|c| c.is_control() || c.is_whitespace() || c == '\\')
        && url::Url::parse(url).is_ok_and(|u| {
            u.scheme() == "https"
                && u.host_str().is_some()
                && u.username().is_empty()
                && u.password().is_none()
        })
}
impl ResourceAction {
    pub fn validate(&self) -> Result<()> {
        match self {
            Self::None {} => Ok(()),
            Self::ExternalLink { url } => require(
                https(url) && url.len() <= 2048,
                "外部链接必须为有效 HTTPS 地址",
            ),
            Self::InternalPage { page, object_id } => require(
                matches!(page.as_str(), "feed" | "resources" | "legal" | "post")
                    && ((page == "post") == object_id.is_some()),
                "请选择支持的本站页面",
            ),
            Self::Phone { number } => require(
                (3..=30).contains(&number.len())
                    && number
                        .chars()
                        .all(|c| c.is_ascii_digit() || "+ -()".contains(c)),
                "电话号码无效",
            ),
            Self::CopyText { text: v } => text(v, 1, 2000).map(|_| ()),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceInput {
    pub id: Option<Uuid>,
    #[serde(default)]
    pub revision: i64,
    pub kind: String,
    pub title: String,
    #[serde(default)]
    pub content: String,
    pub image_ref: Option<ImageRef>,
    #[serde(default)]
    pub action: ResourceAction,
    pub active: bool,
    #[serde(default)]
    pub sort_order: i32,
    pub starts_at: Option<chrono::DateTime<chrono::Utc>>,
    pub ends_at: Option<chrono::DateTime<chrono::Utc>>,
}
impl ResourceInput {
    pub fn validate(&self) -> Result<()> {
        require(
            matches!(
                self.kind.as_str(),
                "banner" | "qrcode" | "service" | "legal" | "tag" | "blocked_word"
            ),
            "资源类型无效",
        )?;
        text(&self.title, 1, 200)?;
        require(
            self.content.len() <= 20000 && self.revision >= 0,
            "资源内容或修订无效",
        )?;
        require(
            self.id.is_some() || self.revision == 0,
            "新资源不能指定旧修订",
        )?;
        require(
            self.starts_at.zip(self.ends_at).is_none_or(|(a, b)| a < b),
            "失效时间须晚于生效时间",
        )?;
        self.action.validate()?;
        require(
            !matches!(self.kind.as_str(), "banner" | "qrcode") || self.image_ref.is_some(),
            "请选择展示图片",
        )?;
        require(
            matches!(self.kind.as_str(), "banner" | "qrcode") || self.image_ref.is_none(),
            "此类型不需要图片",
        )?;
        require(
            !matches!(self.kind.as_str(), "tag" | "blocked_word")
                || matches!(self.action, ResourceAction::None {}),
            "标签与屏蔽词不需要跳转动作",
        )
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn obsolete_image_references_are_rejected() {
        let id = Uuid::new_v4();
        for v in [
            serde_json::json!({"kind":"external","url":"https://example.test/image"}),
            serde_json::json!({"kind":"school_media","school_id":id,"id":id}),
        ] {
            assert!(serde_json::from_value::<ImageRef>(v).is_err());
        }
        assert!(
            serde_json::from_value::<ImageRef>(serde_json::json!({"kind":"asset","id":id})).is_ok()
        );
    }
    #[test]
    fn actions_are_explicit_and_links_cannot_escape() {
        for bad in [
            "//evil.test",
            "https://good.test@evil.test",
            "https://good.test\\evil",
            "/api/v1/schools/other/media/x",
        ] {
            assert!(!https(bad));
        }
        assert!(
            serde_json::from_value::<ResourceAction>(
                serde_json::json!({"kind":"internal_page","page":"/arbitrary","object_id":null})
            )
            .unwrap()
            .validate()
            .is_err()
        );
        assert!(
            serde_json::from_value::<ResourceAction>(serde_json::json!({"kind":"none","url":"x"}))
                .is_err()
        );
    }
}
