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
    pub group_id: Uuid,
    pub group_size: i16,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    pub sold: i64,
    pub remaining: i64,
    pub cancelled: i64,
    pub picked_up: i64,
    pub free_numbers: Vec<i16>,
}

/// ออกเลขบัตรให้คำสั่งซื้อ 1 คน (group_id = order_id)
pub async fn issue_ticket(
    pool: &PgPool,
    total: i16,
    order_id: Uuid,
    input: &NewOrderInput,
    slip_ref: &str,
    slip_path: &str,
) -> Result<Order, AppError> {
    let mut orders = issue_tickets(
        pool,
        total,
        order_id,
        std::slice::from_ref(input),
        slip_ref,
        slip_path,
    )
    .await?;
    Ok(orders.remove(0))
}

/// ออกเลขบัตรให้ทุกคนในกลุ่มพร้อมกัน (ได้ทั้งหมดหรือไม่ได้เลย) ทำทีละคำสั่งด้วย advisory lock
/// แต่ละคนได้เลขว่างที่น้อยที่สุดตามลำดับที่กรอก แถวแรกใช้ group_id เป็น id
pub async fn issue_tickets(
    pool: &PgPool,
    total: i16,
    group_id: Uuid,
    inputs: &[NewOrderInput],
    slip_ref: &str,
    slip_path: &str,
) -> Result<Vec<Order>, AppError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock($1)")
        .bind(ISSUE_LOCK)
        .execute(&mut *tx)
        .await?;

    if !settings::read_state(&mut tx).await?.open {
        return Err(AppError::SalesClosed);
    }

    let student_ids: Vec<&str> = inputs.iter().map(|i| i.student_id.as_str()).collect();
    let existing: Option<(String, i16)> = sqlx::query_as(
        "SELECT student_id, ticket_no FROM orders
         WHERE student_id = ANY($1) AND status = 'active'
         ORDER BY array_position($1, student_id) LIMIT 1",
    )
    .bind(&student_ids)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some((student_id, ticket_no)) = existing {
        return Err(AppError::AlreadyHasTicket {
            ticket_no,
            student_id,
        });
    }

    let slip_used: bool =
        sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM orders WHERE slip_ref = $1)")
            .bind(slip_ref)
            .fetch_one(&mut *tx)
            .await?;
    if slip_used {
        return Err(AppError::DuplicateSlip);
    }

    let free: Vec<i16> = sqlx::query_scalar(
        "SELECT g::smallint FROM generate_series(1, $1::int) g
         WHERE NOT EXISTS (SELECT 1 FROM orders WHERE status = 'active' AND ticket_no = g)
         ORDER BY g LIMIT $2",
    )
    .bind(total as i32)
    .bind(inputs.len() as i64)
    .fetch_all(&mut *tx)
    .await?;
    if free.len() < inputs.len() {
        return Err(AppError::SoldOut);
    }

    let mut orders = Vec::with_capacity(inputs.len());
    for (i, (input, ticket_no)) in inputs.iter().zip(free).enumerate() {
        let id = if i == 0 { group_id } else { Uuid::new_v4() };
        let order: Order = sqlx::query_as(
            "INSERT INTO orders (id, ticket_no, status, name, student_id, phone, slip_path, slip_ref,
                                 group_id, group_size)
             VALUES ($1, $2, 'active', $3, $4, $5, $6, $7, $8, $9) RETURNING *",
        )
        .bind(id)
        .bind(ticket_no)
        .bind(&input.name)
        .bind(&input.student_id)
        .bind(&input.phone)
        .bind(slip_path)
        .bind(slip_ref)
        .bind(group_id)
        .bind(inputs.len() as i16)
        .fetch_one(&mut *tx)
        .await?;
        orders.push(order);
    }
    tx.commit().await?;
    Ok(orders)
}

/// บัตรที่ยังใช้งานได้ในกลุ่มเดียวกัน เรียงตามเลขบัตร
pub async fn list_group(pool: &PgPool, group_id: Uuid) -> Result<Vec<Order>, AppError> {
    Ok(sqlx::query_as(
        "SELECT * FROM orders WHERE group_id = $1 AND status = 'active' ORDER BY ticket_no",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?)
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
    // Postgres ไม่รับ NUL ในข้อความ ตัดทิ้งก่อนค้น
    let cleaned = q.map(|s| s.replace('\0', ""));
    let q = cleaned.as_deref().map(str::trim).filter(|s| !s.is_empty());
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
