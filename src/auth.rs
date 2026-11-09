use crate::{AppState, error::AppError};
use axum::{extract::FromRequestParts, http::request::Parts};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TOKEN_HOURS: i64 = 12;

/// ล็อกอินผิดได้กี่ครั้งภายในช่วงเวลา ก่อนถูกให้รอ
const MAX_LOGIN_FAILURES: usize = 5;
const LOGIN_WINDOW: Duration = Duration::from_secs(60);

/// นับการล็อกอินผิดรวมทั้งระบบ (ไม่แยกตาม IP เพราะหลัง tunnel/proxy ทุกคำขอมาจาก IP เดียวกัน)
#[derive(Debug, Default)]
pub struct LoginLimiter {
    failures: Mutex<VecDeque<Instant>>,
}

impl LoginLimiter {
    fn prune(q: &mut VecDeque<Instant>, now: Instant) {
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) >= LOGIN_WINDOW)
        {
            q.pop_front();
        }
    }

    /// Err ถ้าผิดครบจำนวนแล้ว คืนจำนวนวินาทีที่ต้องรอ
    pub fn check(&self) -> Result<(), AppError> {
        let now = Instant::now();
        let mut q = self.failures.lock().expect("limiter lock");
        Self::prune(&mut q, now);
        if q.len() >= MAX_LOGIN_FAILURES {
            let oldest = *q.front().expect("non-empty");
            let wait = LOGIN_WINDOW.saturating_sub(now.duration_since(oldest));
            return Err(AppError::TooManyAttempts {
                retry_after_secs: wait.as_secs().max(1),
            });
        }
        Ok(())
    }

    pub fn record_failure(&self) {
        let now = Instant::now();
        let mut q = self.failures.lock().expect("limiter lock");
        Self::prune(&mut q, now);
        q.push_back(now);
    }

    pub fn reset(&self) {
        self.failures.lock().expect("limiter lock").clear();
    }
}

/// ตัวนับรูปผีรับได้ไม่เกินเท่านี้ต่อวินาที (รวมทั้งระบบ) กันคนยิงรัวจนกระทบฐานข้อมูล
const MAX_SHOTS_PER_SEC: usize = 30;

/// จำกัดอัตรา POST /api/ghost-shots แบบหน้าต่าง 1 วินาที รวมทั้งระบบ
#[derive(Debug, Default)]
pub struct ShotLimiter {
    hits: Mutex<VecDeque<Instant>>,
}

impl ShotLimiter {
    pub fn check(&self) -> Result<(), AppError> {
        let now = Instant::now();
        let mut q = self.hits.lock().expect("limiter lock");
        while q
            .front()
            .is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(1))
        {
            q.pop_front();
        }
        if q.len() >= MAX_SHOTS_PER_SEC {
            return Err(AppError::TooManyAttempts {
                retry_after_secs: 1,
            });
        }
        q.push_back(now);
        Ok(())
    }
}

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
