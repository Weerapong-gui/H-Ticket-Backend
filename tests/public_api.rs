mod common;
use common::*;
use serde_json::json;
use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
async fn healthz_returns_ok(pool: PgPool) {
    let app = app(pool, test_config());
    let (status, body) = send(&app, get("/healthz")).await;
    assert_eq!(status, 200);
    assert_eq!(body, json!({"status": "ok"}));
}
