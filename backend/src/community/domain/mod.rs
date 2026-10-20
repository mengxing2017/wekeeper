use crate::business::{Result, require, text};
use serde::Deserialize;
use uuid::Uuid;
#[derive(Deserialize)]
pub struct PostInput {
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub revision: Option<i64>,
}
impl PostInput {
    pub fn validate(&self, school: Uuid) -> Result<()> {
        text(&self.title, 1, 200)?;
        text(&self.content, 1, 20000)?;
        require(
            self.images.len() <= 9 && self.tags.len() <= 10,
            "图片或标签过多",
        )?;
        for image in &self.images {
            require(
                image.starts_with(&format!("/api/v1/schools/{school}/media/")),
                "图片必须属于当前学校",
            )?;
        }
        for tag in &self.tags {
            text(tag, 1, 50)?;
        }
        Ok(())
    }
}
#[derive(Deserialize)]
pub struct CommentInput {
    pub content: String,
    pub parent_id: Option<Uuid>,
}
