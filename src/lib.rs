pub mod config;
pub mod db;
pub mod error;
pub mod routes;
pub mod slip;
pub mod storage;
pub mod validate;

use axum::{
    Json, Router,
    extract::DefaultBodyLimit,
    http::HeaderValue,
    routing::{get, post},
};
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
        .route("/api/sales", get(routes::public::sales))
        .route(
            "/api/orders",
            post(routes::public::create_order).layer(DefaultBodyLimit::max(6 * 1024 * 1024)),
        )
        .route("/api/tickets/{student_id}", get(routes::public::ticket))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
