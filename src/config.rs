use anyhow::{Context, bail};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub admin_password: String,
    pub jwt_secret: String,
    pub total_tickets: i16,
    pub slip_dir: PathBuf,
    pub cors_origin: String,
    pub port: u16,
    pub contact_info: String,
}

fn var_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn required(key: &str) -> anyhow::Result<String> {
    let v = std::env::var(key).unwrap_or_default();
    if v.trim().is_empty() {
        bail!("ต้องตั้งค่า env {key}");
    }
    Ok(v)
}

pub const MIN_ADMIN_PASSWORD_CHARS: usize = 12;
pub const MIN_JWT_SECRET_CHARS: usize = 32;

/// กันการเผลอใช้รหัสสั้นบนเซิร์ฟเวอร์จริง (นับเป็นตัวอักษร)
pub fn check_secrets(admin_password: &str, jwt_secret: &str) -> anyhow::Result<()> {
    if admin_password.chars().count() < MIN_ADMIN_PASSWORD_CHARS {
        bail!("ADMIN_PASSWORD ต้องยาวอย่างน้อย {MIN_ADMIN_PASSWORD_CHARS} ตัวอักษร");
    }
    if jwt_secret.chars().count() < MIN_JWT_SECRET_CHARS {
        bail!(
            "JWT_SECRET ต้องยาวอย่างน้อย {MIN_JWT_SECRET_CHARS} ตัวอักษร (สร้างด้วย `openssl rand -hex 32`)"
        );
    }
    Ok(())
}

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        let total_tickets: i16 = var_or("TOTAL_TICKETS", "600")
            .parse()
            .context("TOTAL_TICKETS ต้องเป็นตัวเลข")?;
        if !(1..=999).contains(&total_tickets) {
            bail!("TOTAL_TICKETS ต้องอยู่ระหว่าง 1-999");
        }
        let admin_password = required("ADMIN_PASSWORD")?;
        let jwt_secret = required("JWT_SECRET")?;
        check_secrets(&admin_password, &jwt_secret)?;
        Ok(Config {
            database_url: required("DATABASE_URL")?,
            admin_password,
            jwt_secret,
            total_tickets,
            slip_dir: PathBuf::from(var_or("SLIP_DIR", "/data/slips")),
            cors_origin: var_or("CORS_ORIGIN", "http://localhost:3000"),
            port: var_or("PORT", "8080").parse().context("PORT ต้องเป็นตัวเลข")?,
            contact_info: var_or("CONTACT_INFO", ""),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_admin_password_rejected() {
        let e = check_secrets("short-pass1", &"s".repeat(32)).unwrap_err();
        assert!(e.to_string().contains("ADMIN_PASSWORD"));
    }

    #[test]
    fn short_jwt_secret_rejected() {
        let e = check_secrets("long-enough-pw", &"s".repeat(31)).unwrap_err();
        assert!(e.to_string().contains("JWT_SECRET"));
    }

    #[test]
    fn long_enough_secrets_ok() {
        assert!(check_secrets("123456789012", &"s".repeat(32)).is_ok());
        // นับเป็นตัวอักษร ไม่ใช่ byte
        assert!(check_secrets("รหัสผ่านไทยยาวพอแล้ว", &"s".repeat(32)).is_ok());
        assert!(check_secrets("ก".repeat(11).as_str(), &"s".repeat(32)).is_err());
    }
}
