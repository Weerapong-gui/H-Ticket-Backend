pub mod config;
pub mod error;
pub mod slip;

use axum::{Json, Router, http::HeaderValue, routing::get};
use config::Config;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tower_http::{
    cors::{Any, CorsLayer},
    trace::TraceLayer,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub config: Arc<Config>,
}

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub fn build_app(state: AppState) -> Router {
    let cors = match state.config.cors_origin.parse::<HeaderValue>() {
        Ok(origin) => CorsLayer::new()
            .allow_origin(origin)
            .allow_methods(Any)
            .allow_headers(Any),
        Err(_) => CorsLayer::new(),
    };
    Router::new()
        .route("/healthz", get(healthz))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
