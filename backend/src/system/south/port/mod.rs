use std::future::Future;

pub trait ReadinessProbe: Clone + Send + Sync + 'static {
    fn is_ready(&self) -> impl Future<Output = bool> + Send;
}
