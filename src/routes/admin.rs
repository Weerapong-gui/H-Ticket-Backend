use crate::AppState;
use crate::auth::{AdminAuth, issue_token, password_matches};
use crate::db::{orders, orders::Order, rejected, settings, settings::SalesState};
use crate::error::AppError;
use crate::storage::SlipStore;
use crate::validate::ImageKind;
use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::header,
    response::{IntoResponse, Response},
};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

fn body<T>(b: Result<Json<T>, JsonRejection>) -> Result<T, AppError> {
    b.map(|Json(v)| v)
        .map_err(|_| AppError::InvalidInput("ข้อมูล JSON ไม่ถูกต้อง".into()))
}

fn parse_id(s: &str) -> Result<Uuid, AppError> {
    Uuid::parse_str(s).map_err(|_| AppError::InvalidInput("รหัสรายการไม่ถูกต้อง".into()))
}

fn order_json(o: &Order) -> Value {
    serde_json::to_value(o).expect("Order serializes")
}

async fn slip_response(st: &AppState, rel: &str) -> Result<Response, AppError> {
    let bytes = SlipStore::new(&st.config.slip_dir)
        .read(rel)
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                AppError::NotFound
            } else {
                e.into()
            }
        })?;
    let mime = rel
        .rsplit('.')
        .next()
        .and_then(ImageKind::from_ext)
        .map_or("application/octet-stream", ImageKind::mime);
    Ok(([(header::CONTENT_TYPE, mime)], bytes).into_response())
}

#[derive(Deserialize)]
pub struct LoginBody {
    password: String,
}

pub async fn login(
    State(st): State<AppState>,
    b: Result<Json<LoginBody>, JsonRejection>,
) -> Result<Json<Value>, AppError> {
    let b = body(b)?;
    st.login_limiter.check()?;
    if !password_matches(&b.password, &st.config.admin_password) {
        st.login_limiter.record_failure();
        return Err(AppError::Unauthorized);
    }
    st.login_limiter.reset();
    Ok(Json(json!({ "token": issue_token(&st.config.jwt_secret) })))
}

pub async fn stats(_: AdminAuth, State(st): State<AppState>) -> Result<Json<Value>, AppError> {
    let s = orders::stats(&st.pool, st.config.total_tickets).await?;
    let sales = settings::get_sales_state(&st.pool).await?;
    let mut v = serde_json::to_value(&s).expect("Stats serializes");
    v["open"] = json!(sales.open);
    v["closed_forever"] = json!(sales.closed_forever);
    v["ghost_shots"] = json!(settings::ghost_shots(&st.pool).await?);
    Ok(Json(v))
}

#[derive(Deserialize)]
pub struct ListQuery {
    q: Option<String>,
    status: Option<String>,
}

pub async fn list_orders(
    _: AdminAuth,
    State(st): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    let list = orders::list_orders(&st.pool, q.q.as_deref(), q.status.as_deref()).await?;
    Ok(Json(list.iter().map(order_json).collect()))
}

pub async fn order_slip(
    _: AdminAuth,
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let o = orders::get_order(&st.pool, parse_id(&id)?).await?;
    slip_response(&st, &o.slip_path).await
}

#[derive(Deserialize)]
pub struct CancelBody {
    reason: String,
}

pub async fn cancel(
    _: AdminAuth,
    State(st): State<AppState>,
    Path(id): Path<String>,
    b: Result<Json<CancelBody>, JsonRejection>,
) -> Result<Json<Value>, AppError> {
    let id = parse_id(&id)?;
    let reason = body(b)?.reason;
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(AppError::InvalidInput("กรุณาระบุเหตุผลที่ยกเลิก".into()));
    }
    Ok(Json(order_json(
        &orders::cancel_order(&st.pool, id, reason).await?,
    )))
}

#[derive(Deserialize)]
pub struct PickupBody {
    picked_up: bool,
}

pub async fn pickup(
    _: AdminAuth,
    State(st): State<AppState>,
    Path(id): Path<String>,
    b: Result<Json<PickupBody>, JsonRejection>,
) -> Result<Json<Value>, AppError> {
    let id = parse_id(&id)?;
    let picked = body(b)?.picked_up;
    Ok(Json(order_json(
        &orders::set_picked_up(&st.pool, id, picked).await?,
    )))
}

#[derive(Deserialize)]
pub struct SalesBody {
    open: bool,
}

pub async fn set_sales(
    _: AdminAuth,
    State(st): State<AppState>,
    b: Result<Json<SalesBody>, JsonRejection>,
) -> Result<Json<SalesState>, AppError> {
    Ok(Json(
        settings::set_sales_open(&st.pool, body(b)?.open).await?,
    ))
}

#[derive(Deserialize)]
pub struct TotalBody {
    total: i64,
}

/// แก้จำนวนบัตรทั้งหมด แล้วตอบสถิติชุดใหม่ (หน้าแดชบอร์ดใช้แทนการโหลดซ้ำ)
pub async fn set_total(
    _: AdminAuth,
    State(st): State<AppState>,
    b: Result<Json<TotalBody>, JsonRejection>,
) -> Result<Json<Value>, AppError> {
    settings::set_total(&st.pool, body(b)?.total).await?;
    stats(AdminAuth, State(st)).await
}

pub async fn close_forever(
    _: AdminAuth,
    State(st): State<AppState>,
) -> Result<Json<SalesState>, AppError> {
    Ok(Json(settings::close_forever(&st.pool).await?))
}

pub async fn list_rejected(
    _: AdminAuth,
    State(st): State<AppState>,
) -> Result<Json<Vec<rejected::RejectedAttempt>>, AppError> {
    Ok(Json(rejected::list(&st.pool).await?))
}

pub async fn rejected_slip(
    _: AdminAuth,
    State(st): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let r = rejected::get(&st.pool, parse_id(&id)?).await?;
    let path = r.slip_path.ok_or(AppError::NotFound)?;
    slip_response(&st, &path).await
}
