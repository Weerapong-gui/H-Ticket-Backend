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

impl Config {
    pub fn from_env() -> anyhow::Result<Config> {
        let total_tickets: i16 = var_or("TOTAL_TICKETS", "600")
            .parse()
            .context("TOTAL_TICKETS ต้องเป็นตัวเลข")?;
        if !(1..=999).contains(&total_tickets) {
            bail!("TOTAL_TICKETS ต้องอยู่ระหว่าง 1-999");
        }
        Ok(Config {
            database_url: required("DATABASE_URL")?,
            admin_password: required("ADMIN_PASSWORD")?,
            jwt_secret: required("JWT_SECRET")?,
            total_tickets,
            slip_dir: PathBuf::from(var_or("SLIP_DIR", "/data/slips")),
            cors_origin: var_or("CORS_ORIGIN", "http://localhost:3000"),
            port: var_or("PORT", "8080").parse().context("PORT ต้องเป็นตัวเลข")?,
            contact_info: var_or("CONTACT_INFO", ""),
        })
    }
}
