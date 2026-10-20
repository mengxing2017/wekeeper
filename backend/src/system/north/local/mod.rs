use crate::system::{domain::SystemInfo, south::port::ReadinessProbe};

#[derive(Clone)]
pub struct SystemService<P> {
    probe: P,
}

impl<P: ReadinessProbe> SystemService<P> {
    pub fn new(probe: P) -> Self {
        Self { probe }
    }

    pub fn info(&self) -> SystemInfo {
        SystemInfo::default()
    }

    pub async fn ready(&self) -> bool {
        self.probe.is_ready().await
    }
}
