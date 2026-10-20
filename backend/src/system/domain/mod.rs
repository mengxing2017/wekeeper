#[derive(Debug, PartialEq, Eq)]
pub struct SystemInfo {
    pub application: &'static str,
    pub version: &'static str,
    pub stage: &'static str,
}

impl Default for SystemInfo {
    fn default() -> Self {
        Self {
            application: "wekeeper",
            version: env!("CARGO_PKG_VERSION"),
            stage: "business",
        }
    }
}
