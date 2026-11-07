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

use h_ticket_backend::db::rejected;

const SID: &str = "6831501178";

#[sqlx::test(migrations = "./migrations")]
async fn post_order_issues_ticket(pool: PgPool) {
    open_sales(&pool).await;
    let cfg = test_config();
    let dir = cfg.slip_dir.clone();
    let app = app(pool, cfg);
    let (status, body) = send(
        &app,
        multipart_order("สมชาย", SID, "0812345678", &slip_png("R1")),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    assert_eq!(body["ticket_label"], "001");
    assert_eq!(body["ticket_no"], 1);
    assert!(body.get("phone").is_none());
    let files: Vec<_> = std::fs::read_dir(dir.join("orders")).unwrap().collect();
    assert_eq!(files.len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn lookup_ticket_with_spaces(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    send(
        &app,
        multipart_order("สมชาย", &format!(" {SID} "), "0812345678", &slip_png("R1")),
    )
    .await;
    let (status, body) = send(&app, get(&format!("/api/tickets/%20{SID}%20"))).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["ticket_label"], "001");
    assert_eq!(body["picked_up"], false);
    assert!(body.get("phone").is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn lookup_unknown_404(pool: PgPool) {
    let app = app(pool, test_config());
    let (status, body) = send(&app, get("/api/tickets/6899999999")).await;
    assert_eq!(status, 404);
    assert_eq!(body["code"], "not_found");
}

#[sqlx::test(migrations = "./migrations")]
async fn non_slip_image_rejected_and_logged(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool.clone(), test_config());
    let (status, body) = send(
        &app,
        multipart_order(
            "สมชาย",
            SID,
            "0812345678",
            &png_of_qr("https://example.com"),
        ),
    )
    .await;
    assert_eq!(status, 422);
    assert_eq!(body["code"], "invalid_slip");
    assert_eq!(body["contact"], "LINE @hticket");
    assert!(body["message"].as_str().unwrap().contains("กรุณาติดต่อทีมงาน"));
    let rows = rejected::list(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].reason, "invalid_slip");
    assert!(rows[0].slip_path.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_slip_409_and_logged(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool.clone(), test_config());
    send(
        &app,
        multipart_order("A", SID, "0812345678", &slip_png("R1")),
    )
    .await;
    let (status, body) = send(
        &app,
        multipart_order("B", "6800000002", "0812345678", &slip_png("R1")),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["code"], "duplicate_slip");
    assert_eq!(body["contact"], "LINE @hticket");
    let rows = rejected::list(&pool).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].slip_ref.as_deref(), Some("R1"));
}

#[sqlx::test(migrations = "./migrations")]
async fn already_has_ticket_returns_existing_number(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool.clone(), test_config());
    send(
        &app,
        multipart_order("A", SID, "0812345678", &slip_png("R1")),
    )
    .await;
    let (status, body) = send(
        &app,
        multipart_order("A", SID, "0812345678", &slip_png("R2")),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["code"], "already_has_ticket");
    assert_eq!(body["ticket_label"], "001");
    assert_eq!(
        rejected::list(&pool).await.unwrap()[0].reason,
        "already_has_ticket"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn sales_closed_403_not_logged(pool: PgPool) {
    let app = app(pool.clone(), test_config());
    let (status, body) = send(
        &app,
        multipart_order("A", SID, "0812345678", &slip_png("R1")),
    )
    .await;
    assert_eq!(status, 403);
    assert_eq!(body["code"], "sales_closed");
    assert!(rejected::list(&pool).await.unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn invalid_input_400(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    let (status, body) = send(
        &app,
        multipart_order("A", "123", "0812345678", &slip_png("R1")),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["code"], "invalid_input");
    let (status, _) = send(
        &app,
        multipart_order("A", SID, "0812345678", b"%PDF-1.7 hello"),
    )
    .await;
    assert_eq!(status, 400);
}

#[sqlx::test(migrations = "./migrations")]
async fn oversized_upload_rejected(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    let mut big = slip_png("R1");
    big.resize(7 * 1024 * 1024, 0);
    let (status, body) = send(&app, multipart_order("A", SID, "0812345678", &big)).await;
    assert!(status == 400 || status == 413, "{status}");
    assert_eq!(body["code"], "invalid_input");
}

#[sqlx::test(migrations = "./migrations")]
async fn sales_endpoint_counts(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    send(
        &app,
        multipart_order("A", SID, "0812345678", &slip_png("R1")),
    )
    .await;
    let (status, body) = send(&app, get("/api/sales")).await;
    assert_eq!(status, 200);
    assert_eq!(
        body,
        json!({"open": true, "closed_forever": false, "total": 600, "sold": 1, "remaining": 599})
    );
}
