use super::{ISSUE_LOCK, settings};
use crate::error::AppError;
use crate::validate::NewOrderInput;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Order {
    pub id: Uuid,
    pub ticket_no: i16,
    pub status: String,
    pub name: String,
    pub student_id: String,
    pub phone: String,
    pub slip_path: String,
    pub slip_ref: String,
    pub created_at: DateTime<Utc>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancel_reason: Option<String>,
    pub picked_up_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub sold: i64,
    pub remaining: i64,
    pub cancelled: i64,
    pub picked_up: i64,
    pub free_numbers: Vec<i16>,
}

/// ออกเลขบัตรให้คำสั่งซื้อ ทำทีละคำสั่งด้วย advisory lock
/// คนถัดไปได้เลขว่างที่น้อยที่สุด
pub async fn issue_ticket(
    pool: &PgPool,
    total: i16,
    order_id: Uuid,
    input: &NewOrderInput,
    slip_ref: &str,
    slip_path: &str,
) -> Result<Order, AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ISSUE_LOCK)
        .execute(&mut *tx)
        .await?;

    if !settings::read_state(&mut tx).await?.open {
        return Err(AppError::SalesClosed);
    }

    let existing: Option<i16> = sqlx::query_scalar(
        "SELECT ticket_no FROM orders WHERE student_id = $1 AND status = 'active'",
    )
    .bind(&input.student_id)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(ticket_no) = existing {
        return Err(AppError::AlreadyHasTicket { ticket_no });
    }

    let slip_used: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM orders WHERE slip_ref = $1)")
            .bind(slip_ref)
            .fetch_one(&mut *tx)
            .await?;
    if slip_used {
        return Err(AppError::DuplicateSlip);
    }

    let next: Option<i16> = sqlx::query_scalar(
        "SELECT min(g)::smallint FROM generate_series(1, $1::int) g
         WHERE NOT EXISTS (SELECT 1 FROM orders WHERE status = 'active' AND ticket_no = g)",
    )
    .bind(total as i32)
    .fetch_one(&mut *tx)
    .await?;
    let ticket_no = next.ok_or(AppError::SoldOut)?;

    let order: Order = sqlx::query_as(
        "INSERT INTO orders (id, ticket_no, status, name, student_id, phone, slip_path, slip_ref)
         VALUES ($1, $2, 'active', $3, $4, $5, $6, $7) RETURNING *",
    )
    .bind(order_id)
    .bind(ticket_no)
    .bind(&input.name)
    .bind(&input.student_id)
    .bind(&input.phone)
    .bind(slip_path)
    .bind(slip_ref)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(order)
}

pub async fn find_active_by_student(
    pool: &PgPool,
    student_id: &str,
) -> Result<Option<Order>, AppError> {
    Ok(
        sqlx::query_as("SELECT * FROM orders WHERE student_id = $1 AND status = 'active'")
            .bind(student_id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn get_order(pool: &PgPool, id: Uuid) -> Result<Order, AppError> {
    sqlx::query_as("SELECT * FROM orders WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

/// ค้นหาด้วยชื่อ/รหัส/เบอร์ (บางส่วน) หรือเลขบัตร (ถ้าเป็นตัวเลข)
pub async fn list_orders(
    pool: &PgPool,
    q: Option<&str>,
    status: Option<&str>,
) -> Result<Vec<Order>, AppError> {
    let q = q.map(str::trim).filter(|s| !s.is_empty());
    let pattern = q.map(|s| format!("%{s}%"));
    let ticket_no: Option<i16> = q.and_then(|s| s.parse().ok());
    Ok(sqlx::query_as(
        "SELECT * FROM orders
         WHERE ($1::text IS NULL OR status = $1)
           AND ($2::text IS NULL OR name ILIKE $2 OR student_id ILIKE $2 OR phone ILIKE $2
                OR ticket_no = $3)
         ORDER BY ticket_no, created_at",
    )
    .bind(status.filter(|s| !s.is_empty()))
    .bind(pattern)
    .bind(ticket_no)
    .fetch_all(pool)
    .await?)
}

pub async fn cancel_order(pool: &PgPool, id: Uuid, reason: &str) -> Result<Order, AppError> {
    let updated: Option<Order> = sqlx::query_as(
        "UPDATE orders SET status = 'cancelled', cancelled_at = now(), cancel_reason = $2
         WHERE id = $1 AND status = 'active' RETURNING *",
    )
    .bind(id)
    .bind(reason)
    .fetch_optional(pool)
    .await?;
    match updated {
        Some(o) => Ok(o),
        None => {
            get_order(pool, id).await?;
            Err(AppError::Conflict("บัตรนี้ถูกยกเลิกไปแล้ว".into()))
        }
    }
}

pub async fn set_picked_up(pool: &PgPool, id: Uuid, picked_up: bool) -> Result<Order, AppError> {
    let updated: Option<Order> = sqlx::query_as(
        "UPDATE orders SET picked_up_at = CASE WHEN $2 THEN COALESCE(picked_up_at, now()) ELSE NULL END
         WHERE id = $1 AND status = 'active' RETURNING *",
    )
    .bind(id)
    .bind(picked_up)
    .fetch_optional(pool)
    .await?;
    match updated {
        Some(o) => Ok(o),
        None => {
            get_order(pool, id).await?;
            Err(AppError::Conflict("บัตรนี้ถูกยกเลิกไปแล้ว".into()))
        }
    }
}

pub async fn stats(pool: &PgPool, total: i16) -> Result<Stats, AppError> {
    let (sold, cancelled, picked_up): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE status = 'active'),
                count(*) FILTER (WHERE status = 'cancelled'),
                count(*) FILTER (WHERE status = 'active' AND picked_up_at IS NOT NULL)
         FROM orders",
    )
    .fetch_one(pool)
    .await?;
    let free_numbers: Vec<i16> = sqlx::query_scalar(
        "SELECT g::smallint FROM generate_series(1, $1::int) g
         WHERE NOT EXISTS (SELECT 1 FROM orders WHERE status = 'active' AND ticket_no = g)
         ORDER BY g",
    )
    .bind(total as i32)
    .fetch_all(pool)
    .await?;
    Ok(Stats {
        sold,
        remaining: free_numbers.len() as i64,
        cancelled,
        picked_up,
        free_numbers,
    })
}
