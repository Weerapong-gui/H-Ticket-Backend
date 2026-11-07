mod common;
use axum::body::Body;
use axum::http::Request;
use common::*;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const SID: &str = "6831501178";

fn json_req(method: &str, uri: &str, token: Option<&str>, body: Value) -> Request<Body> {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(t) = token {
        b = b.header("authorization", format!("Bearer {t}"));
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn authed_get(uri: &str, token: &str) -> Request<Body> {
    Request::get(uri)
        .header("authorization", format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap()
}

async fn login(app: &axum::Router) -> String {
    let (status, body) = send(
        app,
        json_req(
            "POST",
            "/api/admin/login",
            None,
            json!({"password": "admin-pass"}),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    body["token"].as_str().unwrap().to_string()
}

async fn buy(app: &axum::Router, sid: &str, slip_ref: &str) -> Value {
    let (status, body) = send(
        app,
        multipart_order("สมชาย ใจดี", sid, "0812345678", &slip_png(slip_ref)),
    )
    .await;
    assert_eq!(status, 201, "{body}");
    body
}

async fn order_id_of(app: &axum::Router, token: &str, sid: &str) -> String {
    let (_, list) = send(
        app,
        authed_get(&format!("/api/admin/orders?q={sid}"), token),
    )
    .await;
    list[0]["id"].as_str().unwrap().to_string()
}

#[sqlx::test(migrations = "./migrations")]
async fn login_wrong_password_401(pool: PgPool) {
    let app = app(pool, test_config());
    let (status, body) = send(
        &app,
        json_req(
            "POST",
            "/api/admin/login",
            None,
            json!({"password": "nope"}),
        ),
    )
    .await;
    assert_eq!(status, 401);
    assert_eq!(body["code"], "unauthorized");
}

#[sqlx::test(migrations = "./migrations")]
async fn login_ok_returns_token(pool: PgPool) {
    let app = app(pool, test_config());
    assert!(!login(&app).await.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_routes_require_token(pool: PgPool) {
    let app = app(pool, test_config());
    let id = "00000000-0000-0000-0000-000000000000";
    let routes = [
        ("GET", "/api/admin/stats".to_string()),
        ("GET", "/api/admin/orders".to_string()),
        ("GET", format!("/api/admin/orders/{id}/slip")),
        ("POST", format!("/api/admin/orders/{id}/cancel")),
        ("POST", format!("/api/admin/orders/{id}/pickup")),
        ("PUT", "/api/admin/sales".to_string()),
        ("POST", "/api/admin/sales/close-forever".to_string()),
        ("GET", "/api/admin/rejected".to_string()),
        ("GET", format!("/api/admin/rejected/{id}/slip")),
    ];
    for (m, uri) in routes {
        for tok in [None, Some("garbage")] {
            let (status, _) = send(&app, json_req(m, &uri, tok, json!({}))).await;
            assert_eq!(status, 401, "{m} {uri} {tok:?}");
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn expired_token_401(pool: PgPool) {
    use jsonwebtoken::{EncodingKey, Header, encode};
    let app = app(pool, test_config());
    let claims = json!({"sub": "admin", "exp": 1_000_000_000u64});
    let tok = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"test-secret"),
    )
    .unwrap();
    let (status, _) = send(&app, authed_get("/api/admin/stats", &tok)).await;
    assert_eq!(status, 401);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_then_lookup_404_and_number_free(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    buy(&app, SID, "R1").await;
    let token = login(&app).await;
    let id = order_id_of(&app, &token, SID).await;
    let (status, body) = send(
        &app,
        json_req(
            "POST",
            &format!("/api/admin/orders/{id}/cancel"),
            Some(&token),
            json!({"reason": "สลิปปลอม"}),
        ),
    )
    .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["status"], "cancelled");
    let (status, _) = send(&app, get(&format!("/api/tickets/{SID}"))).await;
    assert_eq!(status, 404);
    let (_, stats) = send(&app, authed_get("/api/admin/stats", &token)).await;
    assert_eq!(stats["free_numbers"][0], 1);
    assert_eq!(stats["cancelled"], 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_twice_409(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    buy(&app, SID, "R1").await;
    let token = login(&app).await;
    let id = order_id_of(&app, &token, SID).await;
    let uri = format!("/api/admin/orders/{id}/cancel");
    send(
        &app,
        json_req("POST", &uri, Some(&token), json!({"reason": "x"})),
    )
    .await;
    let (status, body) = send(
        &app,
        json_req("POST", &uri, Some(&token), json!({"reason": "x"})),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["code"], "conflict");
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_requires_reason(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    buy(&app, SID, "R1").await;
    let token = login(&app).await;
    let id = order_id_of(&app, &token, SID).await;
    let (status, _) = send(
        &app,
        json_req(
            "POST",
            &format!("/api/admin/orders/{id}/cancel"),
            Some(&token),
            json!({"reason": " "}),
        ),
    )
    .await;
    assert_eq!(status, 400);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_unknown_id_404(pool: PgPool) {
    let app = app(pool, test_config());
    let token = login(&app).await;
    let uri = "/api/admin/orders/00000000-0000-0000-0000-000000000000/cancel";
    let (status, _) = send(
        &app,
        json_req("POST", uri, Some(&token), json!({"reason": "x"})),
    )
    .await;
    assert_eq!(status, 404);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_bad_uuid_400_json(pool: PgPool) {
    let app = app(pool, test_config());
    let token = login(&app).await;
    let (status, body) = send(
        &app,
        json_req(
            "POST",
            "/api/admin/orders/not-a-uuid/cancel",
            Some(&token),
            json!({"reason": "x"}),
        ),
    )
    .await;
    assert_eq!(status, 400);
    assert_eq!(body["code"], "invalid_input");
}

#[sqlx::test(migrations = "./migrations")]
async fn pickup_toggle(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    buy(&app, SID, "R1").await;
    let token = login(&app).await;
    let id = order_id_of(&app, &token, SID).await;
    let (status, _) = send(
        &app,
        json_req(
            "POST",
            &format!("/api/admin/orders/{id}/pickup"),
            Some(&token),
            json!({"picked_up": true}),
        ),
    )
    .await;
    assert_eq!(status, 200);
    let (_, t) = send(&app, get(&format!("/api/tickets/{SID}"))).await;
    assert_eq!(t["picked_up"], true);
}

#[sqlx::test(migrations = "./migrations")]
async fn slip_download_matches_upload(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    buy(&app, SID, "R1").await;
    let token = login(&app).await;
    let id = order_id_of(&app, &token, SID).await;
    let res = app
        .clone()
        .oneshot(authed_get(&format!("/api/admin/orders/{id}/slip"), &token))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.headers()["content-type"], "image/png");
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.to_vec(), slip_png("R1"));
}

#[sqlx::test(migrations = "./migrations")]
async fn search_by_ticket_no_and_name(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    buy(&app, SID, "R1").await;
    buy(&app, "6800000002", "R2").await;
    let token = login(&app).await;
    let (_, all) = send(&app, authed_get("/api/admin/orders", &token)).await;
    assert_eq!(all.as_array().unwrap().len(), 2);
    assert_eq!(all[0]["phone"], "0812345678");
    assert_eq!(all[0]["ticket_label"], "001");
    let (_, by_no) = send(&app, authed_get("/api/admin/orders?q=2", &token)).await;
    assert!(
        by_no
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["ticket_no"] == 2)
    );
    let (_, active) = send(
        &app,
        authed_get("/api/admin/orders?status=cancelled", &token),
    )
    .await;
    assert!(active.as_array().unwrap().is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn close_forever_then_reopen_409(pool: PgPool) {
    let app = app(pool, test_config());
    let token = login(&app).await;
    let (status, s) = send(
        &app,
        json_req(
            "PUT",
            "/api/admin/sales",
            Some(&token),
            json!({"open": true}),
        ),
    )
    .await;
    assert_eq!((status.as_u16(), s["open"].clone()), (200, json!(true)));
    let (status, s) = send(
        &app,
        json_req(
            "POST",
            "/api/admin/sales/close-forever",
            Some(&token),
            json!({}),
        ),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(s, json!({"open": false, "closed_forever": true}));
    let (status, body) = send(
        &app,
        json_req(
            "PUT",
            "/api/admin/sales",
            Some(&token),
            json!({"open": true}),
        ),
    )
    .await;
    assert_eq!(status, 409);
    assert_eq!(body["code"], "presale_closed_forever");
    let (_, stats) = send(&app, authed_get("/api/admin/stats", &token)).await;
    assert_eq!(stats["closed_forever"], true);
    assert_eq!(stats["total"], 600);
}

#[sqlx::test(migrations = "./migrations")]
async fn rejected_list_and_slip(pool: PgPool) {
    open_sales(&pool).await;
    let app = app(pool, test_config());
    let bad = png_of_qr("hello");
    send(&app, multipart_order("A", SID, "0812345678", &bad)).await;
    let token = login(&app).await;
    let (status, list) = send(&app, authed_get("/api/admin/rejected", &token)).await;
    assert_eq!(status, 200);
    assert_eq!(list[0]["reason"], "invalid_slip");
    let id = list[0]["id"].as_str().unwrap();
    let res = app
        .clone()
        .oneshot(authed_get(
            &format!("/api/admin/rejected/{id}/slip"),
            &token,
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(bytes.to_vec(), bad);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_locked_after_5_failures(pool: PgPool) {
    let app = app(pool, test_config());
    for _ in 0..5 {
        let (status, _) = send(
            &app,
            json_req(
                "POST",
                "/api/admin/login",
                None,
                json!({"password": "nope"}),
            ),
        )
        .await;
        assert_eq!(status, 401);
    }
    let (status, body) = send(
        &app,
        json_req(
            "POST",
            "/api/admin/login",
            None,
            json!({"password": "nope"}),
        ),
    )
    .await;
    assert_eq!(status, 429);
    assert_eq!(body["code"], "too_many_attempts");
    // แม้รหัสถูกก็ต้องรอ ไม่งั้นการล็อกไม่มีความหมาย
    let (status, _) = send(
        &app,
        json_req(
            "POST",
            "/api/admin/login",
            None,
            json!({"password": "admin-pass"}),
        ),
    )
    .await;
    assert_eq!(status, 429);
}

#[sqlx::test(migrations = "./migrations")]
async fn successful_login_resets_failures(pool: PgPool) {
    let app = app(pool, test_config());
    for _ in 0..4 {
        send(
            &app,
            json_req(
                "POST",
                "/api/admin/login",
                None,
                json!({"password": "nope"}),
            ),
        )
        .await;
    }
    login(&app).await;
    for _ in 0..4 {
        let (status, _) = send(
            &app,
            json_req(
                "POST",
                "/api/admin/login",
                None,
                json!({"password": "nope"}),
            ),
        )
        .await;
        assert_eq!(status, 401);
    }
}
