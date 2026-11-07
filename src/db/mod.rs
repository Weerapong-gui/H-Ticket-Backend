pub mod orders;
pub mod rejected;
pub mod settings;

/// ล็อกเดียวกันทั้งตอนออกเลขและตอนเปลี่ยนสถานะการขาย ให้ทำงานทีละคำสั่ง
pub(crate) const ISSUE_LOCK: i64 = 7001;
