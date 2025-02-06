mod article;
mod auth;
mod errors;
mod profiles;
mod users;

use crate::config::Config;
use anyhow::Context;
use axum::Router;
pub use errors::Error;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone)]
struct AppState {
    config: Arc<Config>,
    db: PgPool,
}

pub async fn serve(config: Config, db: PgPool) -> anyhow::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let state = AppState {
        config: Arc::new(config),
        db,
    };

    let app = api_router(state.config.clone())
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    axum::serve(listener, app)
        .await
        .context("error running HTTP server")?;

    Ok(())
}

fn api_router(state: Arc<Config>) -> Router<AppState> {
    users::router(state.clone())
        .merge(profiles::router(state.clone()))
        .merge(article::router(state))
}
