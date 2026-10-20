use crate::business::Result;
use uuid::Uuid;
pub trait Store: Clone + Send + Sync + 'static {
    fn put(
        &self,
        s: Uuid,
        owner: Uuid,
        mime: &str,
        content: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Uuid>> + Send;
    fn get(
        &self,
        s: Uuid,
        id: Uuid,
    ) -> impl std::future::Future<Output = Result<(String, Vec<u8>)>> + Send;
}
