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
