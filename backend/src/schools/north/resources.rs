use crate::{
    business::{Actor, Error, Result, require},
    schools::south::{
        adapter::postgres::Repository,
        port::resources::{Command, Store},
    },
};
use serde_json::Value;
use uuid::Uuid;
#[derive(Clone)]
pub struct Service<R: Store = Repository>(pub R);
impl<R: Store> Service<R> {
    pub async fn command(&self, a: &Actor, s: Option<Uuid>, v: Command) -> Result<Value> {
        if let Command::Preview(input) = &v {
            input.validate()?;
        }
        self.0.command(a, s, v).await
    }
    pub async fn upload(&self, a: &Actor, s: Option<Uuid>, b: Vec<u8>) -> Result<Value> {
        let (m, w, h) = crate::media::domain::validate(&b)?;
        self.0.upload(a, s, m, w, h, b).await
    }
    pub async fn qr(&self, a: &Actor, s: Option<Uuid>, url: &str) -> Result<Value> {
        require(
            crate::schools::domain::https(url) && url.len() <= 2048,
            "二维码内容须为 HTTPS 业务链接",
        )?;
        let code = qrcode::QrCode::new(url).map_err(|_| Error::Invalid("二维码内容过长".into()))?;
        let image = code
            .render::<image::Luma<u8>>()
            .min_dimensions(384, 384)
            .build();
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageLuma8(image)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|_| Error::Unavailable)?;
        self.upload(a, s, bytes.into_inner()).await
    }
    pub async fn list(&self, a: &Actor, s: Option<Uuid>) -> Result<Vec<Value>> {
        self.0.media_list(a, s).await
    }
    pub async fn media(
        &self,
        a: &Actor,
        s: Option<Uuid>,
        id: Uuid,
        published: bool,
    ) -> Result<(String, Vec<u8>)> {
        self.0.media(a, s, id, published).await
    }
}
