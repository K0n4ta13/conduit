use anyhow::Context;
use clap::Parser;
use sqlx::postgres::PgPoolOptions;

use conduit::config::Config;
use conduit::http;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    env_logger::init();

    let config = Config::parse();

    let db = PgPoolOptions::new()
        .max_connections(50)
        .connect(&config.database_url)
        .await
        .context("failed to connect to database")?;

    sqlx::migrate!().run(&db).await?;

    http::serve(config, db).await?;

    Ok(())
}
