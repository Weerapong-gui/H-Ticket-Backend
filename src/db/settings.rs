use super::ISSUE_LOCK;
use crate::error::AppError;
use serde::Serialize;
use sqlx::{PgConnection, PgPool};

#[derive(Debug, Clone, Copy, Serialize)]
pub struct SalesState {
    pub open: bool,
    pub closed_forever: bool,
}

pub(crate) async fn read_state(conn: &mut PgConnection) -> Result<SalesState, AppError> {
    let rows: Vec<(String, String)> = sqlx::query_as("SELECT key, value FROM settings")
        .fetch_all(conn)
        .await?;
    let flag = |k: &str| rows.iter().any(|(key, v)| key == k && v == "true");
    let closed_forever = flag("presale_closed_forever");
    Ok(SalesState {
        open: flag("sales_open") && !closed_forever,
        closed_forever,
    })
}

async fn set(conn: &mut PgConnection, key: &str, value: bool) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ($1, $2)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(key)
    .bind(if value { "true" } else { "false" })
    .execute(conn)
    .await?;
    Ok(())
}

/// จำนวนบัตรทั้งหมด: ค่าที่แอดมินตั้งไว้ในตาราง settings ถ้ายังไม่เคยตั้งใช้ค่า default (TOTAL_TICKETS)
pub(crate) async fn read_total(conn: &mut PgConnection, default: i16) -> Result<i16, AppError> {
    let v: Option<String> =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'total_tickets'")
            .fetch_optional(conn)
            .await?;
    Ok(v.and_then(|v| v.parse().ok()).unwrap_or(default))
}

/// แอดมินแก้จำนวนบัตรทั้งหมด มีผลทันที · ห้ามน้อยกว่าที่ขายไปแล้ว
pub async fn set_total(pool: &PgPool, total: i64) -> Result<i16, AppError> {
    if !(1..=999).contains(&total) {
        return Err(AppError::InvalidInput(
            "จำนวนบัตรต้องอยู่ระหว่าง 1-999 ใบ".into(),
        ));
    }
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ISSUE_LOCK)
        .execute(&mut *tx)
        .await?;
    let sold: i64 = sqlx::query_scalar("SELECT count(*) FROM orders WHERE status = 'active'")
        .fetch_one(&mut *tx)
        .await?;
    if total < sold {
        return Err(AppError::Conflict(format!(
            "ขายไปแล้ว {sold} ใบ ตั้งจำนวนบัตรน้อยกว่านี้ไม่ได้"
        )));
    }
    sqlx::query(
        "INSERT INTO settings (key, value) VALUES ('total_tickets', $1)
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(total.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(total as i16)
}

pub async fn get_sales_state(pool: &PgPool) -> Result<SalesState, AppError> {
    let mut conn = pool.acquire().await?;
    read_state(&mut conn).await
}

pub async fn set_sales_open(pool: &PgPool, open: bool) -> Result<SalesState, AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ISSUE_LOCK)
        .execute(&mut *tx)
        .await?;
    let state = read_state(&mut tx).await?;
    if open && state.closed_forever {
        return Err(AppError::PresaleClosedForever);
    }
    set(&mut tx, "sales_open", open).await?;
    let state = read_state(&mut tx).await?;
    tx.commit().await?;
    Ok(state)
}

pub async fn close_forever(pool: &PgPool) -> Result<SalesState, AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ISSUE_LOCK)
        .execute(&mut *tx)
        .await?;
    set(&mut tx, "sales_open", false).await?;
    set(&mut tx, "presale_closed_forever", true).await?;
    let state = read_state(&mut tx).await?;
    tx.commit().await?;
    Ok(state)
}
