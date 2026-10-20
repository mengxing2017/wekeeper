use clap::Parser;
use wekeeper_api::{config::Config, observability, runtime};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    observability::init();
    let config = Config::parse();
    let pool = config.connect().await?;
    let result = runtime::serve(pool.clone(), config.bind_addr).await;
    pool.close().await;
    result?;
    Ok(())
}
