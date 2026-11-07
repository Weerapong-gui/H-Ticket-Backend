#![allow(dead_code)]
use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode},
};
use h_ticket_backend::{AppState, build_app, config::Config};
use http_body_util::BodyExt;
use sqlx::PgPool;
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
    build_app(AppState::new(pool, config))
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

pub fn png_of_qr(payload: &str) -> Vec<u8> {
    use image::{ImageFormat, Luma};
    let img = qrcode::QrCode::new(payload.as_bytes())
        .unwrap()
        .render::<Luma<u8>>()
        .min_dimensions(250, 250)
        .build();
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png).unwrap();
    out.into_inner()
}

/// รูปสลิปปลอมที่มี QR ถูกรูปแบบ
pub fn slip_png(trans_ref: &str) -> Vec<u8> {
    png_of_qr(&h_ticket_backend::slip::encode_slip_payload(trans_ref))
}

const BOUNDARY: &str = "XBOUNDARYX";

pub fn multipart_order(name: &str, student_id: &str, phone: &str, file: &[u8]) -> Request<Body> {
    let mut body = Vec::new();
    for (k, v) in [("name", name), ("student_id", student_id), ("phone", phone)] {
        body.extend_from_slice(
            format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"slip\"; filename=\"slip.png\"\r\nContent-Type: image/png\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    Request::post("/api/orders")
        .header(
            "content-type",
            format!("multipart/form-data; boundary={BOUNDARY}"),
        )
        .body(Body::from(body))
        .unwrap()
}

pub async fn open_sales(pool: &PgPool) {
    h_ticket_backend::db::settings::set_sales_open(pool, true)
        .await
        .unwrap();
}
