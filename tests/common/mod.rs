#![allow(dead_code)]
use axum::{Router, body::Body, http::{Request, StatusCode}};
use h_ticket_backend::{AppState, build_app, config::Config};
use http_body_util::BodyExt;
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;

pub fn test_config() -> Config {
    let dir = tempfile::tempdir().unwrap().keep();
    Config {
        database_url: String::new(),
        admin_password: "admin-pass".into(),
        jwt_secret: "test-secret".into(),
        total_tickets: 600,
        slip_dir: dir,
        cors_origin: "http://localhost:3000".into(),
        port: 0,
        contact_info: "LINE @hticket".into(),
    }
}

pub fn app(pool: PgPool, config: Config) -> Router {
    h_ticket_backend::error::set_contact_info(config.contact_info.clone());
    build_app(AppState { pool, config: Arc::new(config) })
}

pub async fn send(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

pub fn get(uri: &str) -> Request<Body> {
    Request::get(uri).body(Body::empty()).unwrap()
}
