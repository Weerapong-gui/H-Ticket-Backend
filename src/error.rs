use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;
use std::sync::OnceLock;

static CONTACT_INFO: OnceLock<String> = OnceLock::new();

/// เรียกครั้งเดียวตอน start เพื่อให้ error เรื่องสลิปแนบช่องทางติดต่อทีมงานได้
pub fn set_contact_info(s: String) {
    let _ = CONTACT_INFO.set(s);
}

fn contact_info() -> String {
    CONTACT_INFO.get().cloned().unwrap_or_default()
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("{0}")]
    InvalidInput(String),
    #[error("invalid slip")]
    InvalidSlip,
    #[error("duplicate slip")]
    DuplicateSlip,
    #[error("already has ticket")]
    AlreadyHasTicket { student_id: String },
    #[error("sales closed")]
    SalesClosed,
    #[error("sold out")]
    SoldOut,
    #[error("presale closed forever")]
    PresaleClosedForever,
    #[error("not found")]
    NotFound,
    #[error("{0}")]
    Conflict(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("too many attempts")]
    TooManyAttempts { retry_after_secs: u64 },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        AppError::Internal(e.into())
    }
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::InvalidInput(_) => "invalid_input",
            AppError::InvalidSlip => "invalid_slip",
            AppError::DuplicateSlip => "duplicate_slip",
            AppError::AlreadyHasTicket { .. } => "already_has_ticket",
            AppError::SalesClosed => "sales_closed",
            AppError::SoldOut => "sold_out",
            AppError::PresaleClosedForever => "presale_closed_forever",
            AppError::NotFound => "not_found",
            AppError::Conflict(_) => "conflict",
            AppError::Unauthorized => "unauthorized",
            AppError::TooManyAttempts { .. } => "too_many_attempts",
            AppError::Internal(_) => "internal",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            AppError::InvalidInput(_) => StatusCode::BAD_REQUEST,
            AppError::InvalidSlip => StatusCode::UNPROCESSABLE_ENTITY,
            AppError::DuplicateSlip
            | AppError::AlreadyHasTicket { .. }
            | AppError::SoldOut
            | AppError::PresaleClosedForever
            | AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::SalesClosed => StatusCode::FORBIDDEN,
            AppError::NotFound => StatusCode::NOT_FOUND,
            AppError::Unauthorized => StatusCode::UNAUTHORIZED,
            AppError::TooManyAttempts { .. } => StatusCode::TOO_MANY_REQUESTS,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn message(&self) -> String {
        match self {
            AppError::InvalidInput(m) | AppError::Conflict(m) => m.clone(),
            AppError::InvalidSlip => "ไม่พบสลิปโอนเงินที่ถูกต้องในรูปนี้ หากโอนเงินแล้วกรุณาติดต่อทีมงาน".into(),
            AppError::DuplicateSlip => "สลิปนี้ถูกใช้ไปแล้ว หากคิดว่าเป็นความผิดพลาดกรุณาติดต่อทีมงาน".into(),
            AppError::AlreadyHasTicket { student_id } => {
                format!("รหัสนักศึกษา {student_id} จองบัตรไว้แล้ว")
            }
            AppError::SalesClosed => "ขณะนี้ปิดการขายบัตรผ่านเว็บ".into(),
            AppError::SoldOut => "บัตรหมดแล้ว".into(),
            AppError::PresaleClosedForever => "ปิด pre-sale ถาวรแล้ว เปิดขายอีกไม่ได้".into(),
            AppError::NotFound => "ไม่พบข้อมูล".into(),
            AppError::Unauthorized => "กรุณาเข้าสู่ระบบแอดมิน".into(),
            AppError::TooManyAttempts { retry_after_secs } => {
                format!("ใส่รหัสผิดหลายครั้ง กรุณารอ {retry_after_secs} วินาทีแล้วลองใหม่")
            }
            AppError::Internal(_) => "เกิดข้อผิดพลาดในระบบ กรุณาลองใหม่อีกครั้ง".into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Internal(e) = &self {
            tracing::error!(error = ?e, "internal error");
        }
        let mut body = json!({ "code": self.code(), "message": self.message() });
        match &self {
            AppError::InvalidSlip | AppError::DuplicateSlip => {
                body["contact"] = json!(contact_info());
            }
            AppError::TooManyAttempts { retry_after_secs } => {
                body["retry_after_secs"] = json!(retry_after_secs);
            }
            AppError::AlreadyHasTicket { student_id } => {
                body["student_id"] = json!(student_id);
            }
            _ => {}
        }
        (self.status(), Json(body)).into_response()
    }
}
