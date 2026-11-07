pub mod auth;
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
    routing::{get, post, put},
};
use config::Config;
use routes::admin;
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
    pub login_limiter: Arc<auth::LoginLimiter>,
}

impl AppState {
    pub fn new(pool: PgPool, config: Config) -> Self {
        AppState {
            pool,
            config: Arc::new(config),
            login_limiter: Arc::default(),
        }
    }
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
        .route("/api/admin/login", post(admin::login))
        .route("/api/admin/stats", get(admin::stats))
        .route("/api/admin/orders", get(admin::list_orders))
        .route("/api/admin/orders/{id}/slip", get(admin::order_slip))
        .route("/api/admin/orders/{id}/cancel", post(admin::cancel))
        .route("/api/admin/orders/{id}/pickup", post(admin::pickup))
        .route("/api/admin/sales", put(admin::set_sales))
        .route("/api/admin/sales/close-forever", post(admin::close_forever))
        .route("/api/admin/rejected", get(admin::list_rejected))
        .route("/api/admin/rejected/{id}/slip", get(admin::rejected_slip))
        .with_state(state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
}
