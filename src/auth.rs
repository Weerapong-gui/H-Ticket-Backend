use crate::{AppState, error::AppError};
use axum::{extract::FromRequestParts, http::request::Parts};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const TOKEN_HOURS: i64 = 12;

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

pub fn issue_token(secret: &str) -> String {
    let exp = (chrono::Utc::now() + chrono::Duration::hours(TOKEN_HOURS)).timestamp() as usize;
    let claims = Claims {
        sub: "admin".into(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .expect("HS256 encoding cannot fail")
}

pub fn verify_token(secret: &str, token: &str) -> bool {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .is_ok_and(|d| d.claims.sub == "admin")
}

/// เทียบรหัสผ่านโดยใช้เวลาเท่ากันเสมอ (เทียบ SHA-256 ของทั้งสองฝั่ง)
pub fn password_matches(given: &str, expected: &str) -> bool {
    let a = Sha256::digest(given.as_bytes());
    let b = Sha256::digest(expected.as_bytes());
    a.iter()
        .zip(b.iter())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// ใส่ใน handler เพื่อบังคับให้ต้องมี token แอดมิน
pub struct AdminAuth;

impl FromRequestParts<AppState> for AdminAuth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, st: &AppState) -> Result<Self, AppError> {
        let token = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        if verify_token(&st.config.jwt_secret, token.trim()) {
            Ok(AdminAuth)
        } else {
            Err(AppError::Unauthorized)
        }
    }
}
