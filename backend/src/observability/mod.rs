use tracing_subscriber::EnvFilter;

pub fn init() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("wekeeper_api=info,embedded_postgres=info,tower_http=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .json()
        .init();
}
