use crate::error::AppError;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct RejectedAttempt {
    pub id: Uuid,
    pub reason: String,
    pub name: Option<String>,
    pub student_id: Option<String>,
    pub phone: Option<String>,
    pub slip_path: Option<String>,
    pub slip_ref: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &PgPool,
    reason: &str,
    name: Option<&str>,
    student_id: Option<&str>,
    phone: Option<&str>,
    slip_path: Option<&str>,
    slip_ref: Option<&str>,
) -> Result<Uuid, AppError> {
    let id = Uuid::new_v4();
    insert_with_id(pool, id, reason, name, student_id, phone, slip_path, slip_ref).await?;
    Ok(id)
}

#[allow(clippy::too_many_arguments)]
pub async fn insert_with_id(
    pool: &PgPool,
    id: Uuid,
    reason: &str,
    name: Option<&str>,
    student_id: Option<&str>,
    phone: Option<&str>,
    slip_path: Option<&str>,
    slip_ref: Option<&str>,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO rejected_attempts (id, reason, name, student_id, phone, slip_path, slip_ref)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(id)
    .bind(reason)
    .bind(name)
    .bind(student_id)
    .bind(phone)
    .bind(slip_path)
    .bind(slip_ref)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list(pool: &PgPool) -> Result<Vec<RejectedAttempt>, AppError> {
    Ok(sqlx::query_as("SELECT * FROM rejected_attempts ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?)
}

pub async fn get(pool: &PgPool, id: Uuid) -> Result<RejectedAttempt, AppError> {
    sqlx::query_as("SELECT * FROM rejected_attempts WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}
