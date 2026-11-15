use super::super::south::adapter::postgres::Repository;
use crate::business::{Actor, Error, Result, require};
use crate::media::south::port::Store;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn put(&self, a: &Actor, b: Vec<u8>) -> Result<String> {
        a.platform()?;
        require(b.len() <= 5 * 1024 * 1024, "图片不能超过 5 MiB")?;
        let mime = if b.starts_with(b"\x89PNG\r\n\x1a\n") {
            "image/png"
        } else if b.starts_with(&[255, 216, 255]) {
            "image/jpeg"
        } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
            "image/gif"
        } else if b.starts_with(b"RIFF") && b.get(8..12) == Some(b"WEBP") {
            "image/webp"
        } else {
            return Err(Error::Invalid("仅支持 PNG/JPEG/GIF/WebP 图片".into()));
        };
        let id = self.0.put(a.id, mime, b).await?;
        Ok(format!("/api/v1/wecom-distribution/media/{id}"))
    }
    pub async fn get(&self, a: &Actor, id: Uuid) -> Result<(String, Vec<u8>)> {
        a.platform()?;
        self.0.get(id).await
    }
}
