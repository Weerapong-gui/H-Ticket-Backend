# Ticket Issuing Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** สร้าง backend Rust สำหรับขายบัตร pre-sale บ้านผีสิง: รับสลิป → ตรวจ QR → ออกเลขบัตร 001–600 ไม่ซ้ำ + API แอดมิน รันบน Docker Compose

**Architecture:** Axum HTTP server เดียว คุยกับ PostgreSQL ผ่าน sqlx (query แบบ runtime ไม่ใช้ `query!` macro เพื่อให้ build ใน Docker ได้โดยไม่ต้องต่อ DB) การออกเลขทำใน transaction ที่ถือ `pg_advisory_xact_lock` + partial unique index เป็นด่านสุดท้าย รูปสลิปเก็บเป็นไฟล์ใน `SLIP_DIR`

**Tech Stack:** Rust 2024, axum 0.8 (feature `multipart`), tokio, sqlx 0.8 (`runtime-tokio`, `postgres`, `uuid`, `chrono`, `migrate`), tower-http 0.6 (`cors`, `trace`, `limit`), jsonwebtoken 9, rqrr 0.9 + image 0.25, serde/serde_json, uuid v4, chrono, thiserror, anyhow, tracing. Dev: qrcode 0.14, tower 0.5 (`util`), http-body-util

**Spec:** `docs/superpowers/specs/2026-11-07-ticket-issuing-design.md`

## Global Constraints

- เลขบัตร 1..`TOTAL_TICKETS` (ค่าเริ่ม 600), แสดงผล 3 หลัก (`format!("{:03}")`) — API ส่งทั้ง `ticket_no` (int) และ `ticket_label` ("001")
- 1 `student_id` มีบัตร `active` ได้ 1 ใบ; เลขบัตร active ไม่ซ้ำ; `slip_ref` UNIQUE ทั้งตาราง `orders`
- ยกเลิก = `status='cancelled'` ไม่ลบแถว ไม่ลบไฟล์
- คนถัดไปได้เลขว่างน้อยที่สุด
- `presale_closed_forever=true` แล้วย้อนไม่ได้, `sales_open` ต้องเป็น false ตลอด
- รหัสนักศึกษา: ตัวเลข 10 หลัก; เบอร์: 10 หลักขึ้นต้น `0`; ชื่อ: ไม่ว่าง ≤100 ตัวอักษร (นับ char); รูป: JPEG/PNG/WebP จาก magic bytes, ≤5 MB
- Error JSON: `{ "code": "<snake_case>", "message": "<ไทย>", ...extra }`; `invalid_slip`/`duplicate_slip` มี `"contact": CONTACT_INFO` และข้อความมี "กรุณาติดต่อทีมงาน"
- Env: `DATABASE_URL`, `ADMIN_PASSWORD`, `JWT_SECRET`, `TOTAL_TICKETS=600`, `SLIP_DIR=/data/slips`, `CORS_ORIGIN`, `PORT=8080`, `CONTACT_INFO`
- `/api/tickets/{student_id}` ห้ามคืนเบอร์โทร
- JWT HS256 อายุ 12 ชม.
- บันทึก `rejected_attempts` เฉพาะ `invalid_slip`, `duplicate_slip`, `already_has_ticket`

## Prerequisite (ผู้ใช้ทำ)

เครื่อง Mac ยังไม่มี Docker → ติดตั้ง **OrbStack** หรือ **Docker Desktop** ก่อนเริ่ม Task 1 Tests ทั้งหมดที่แตะ DB ใช้ `#[sqlx::test(migrations = "./migrations")]` ซึ่งต้องการ `DATABASE_URL` ชี้ไป Postgres ที่ user มีสิทธิ์ CREATE DATABASE:
`docker compose up -d db` แล้ว `export DATABASE_URL=postgres://hticket:hticket@localhost:5432/hticket`

## Review Focus

1. กดส่งซ้ำสองครั้งพร้อมกัน (รหัสเดียว สลิปต่างกัน) → ได้บัตรใบเดียว อีกคำขอได้ `already_has_ticket` — test ใน Task 4
2. พิมพ์รหัสนักศึกษามีช่องว่างหน้า/หลัง (ทั้งตอนซื้อและตอนค้น) → trim แล้วทำงานปกติ — test ใน Task 3 และ Task 5
3. อัปโหลดไฟล์ใหญ่กว่า 5 MB → ได้ JSON `invalid_input` (หรือ 413) ไม่ใช่ connection พัง — test ใน Task 5
4. รูปถ่ายจอ/รูปจากมือถือขนาดใหญ่ที่ QR เป็นส่วนเล็กของภาพ → ยังอ่านได้ — test ใน Task 2
5. ยกเลิกบัตรที่ยกเลิกไปแล้ว / id ไม่มีอยู่ → 409 / 404 ไม่ใช่ 500 — test ใน Task 6

---

## File Structure

```
Cargo.toml
migrations/0001_init.sql
src/main.rs            -- อ่าน config, สร้าง pool, migrate, serve
src/lib.rs             -- pub mod ...; AppState; build_app()
src/config.rs          -- Config::from_env
src/error.rs           -- AppError + IntoResponse
src/slip.rs            -- QR decode + TLV/CRC ตรวจสลิป
src/validate.rs        -- ตรวจชื่อ/รหัส/เบอร์, ตรวจชนิดรูป
src/storage.rs         -- SlipStore เขียน/ย้าย/อ่านไฟล์
src/db/mod.rs, orders.rs, settings.rs, rejected.rs
src/routes/mod.rs, public.rs, admin.rs
src/auth.rs            -- JWT + extractor AdminAuth
tests/common/mod.rs    -- สร้าง app ทดสอบ, ทำรูป QR, สร้าง multipart
tests/issue.rs, tests/public_api.rs, tests/admin_api.rs
Dockerfile, .dockerignore, docker-compose.yml, .env.example, README.md
```

---

### Task 1: Scaffold, config, migration, DB compose, healthz

**Files:**
- Modify: `Cargo.toml` (ใส่ deps ทั้งหมดตาม Tech Stack; เปลี่ยน `name = "h-ticket-backend"` เพื่อเลี่ยง warning ชื่อ crate)
- Create: `migrations/0001_init.sql`, `src/lib.rs`, `src/config.rs`, `src/error.rs`, `src/main.rs` (เขียนทับ), `docker-compose.yml` (เฉพาะ service `db`), `.env.example`, `.gitignore` (`/target`, `.env`, `/data`), `tests/common/mod.rs`, `tests/public_api.rs`

**Interfaces:**
- Produces:
  - `pub struct Config { pub database_url: String, pub admin_password: String, pub jwt_secret: String, pub total_tickets: i16, pub slip_dir: PathBuf, pub cors_origin: String, pub port: u16, pub contact_info: String }`, `Config::from_env() -> anyhow::Result<Config>` (ADMIN_PASSWORD/JWT_SECRET ว่าง → error)
  - `#[derive(Clone)] pub struct AppState { pub pool: PgPool, pub config: Arc<Config> }`
  - `pub fn build_app(state: AppState) -> axum::Router` (ใส่ CORS จาก `cors_origin`, TraceLayer)
  - `pub enum AppError { InvalidInput(String), InvalidSlip, DuplicateSlip, AlreadyHasTicket { ticket_no: i16 }, SalesClosed, SoldOut, PresaleClosedForever, NotFound, Conflict(String), Unauthorized, Internal(anyhow::Error) }` + `fn code(&self) -> &'static str` + `IntoResponse`
    - status: InvalidInput 400, InvalidSlip 422, DuplicateSlip 409, AlreadyHasTicket 409 (extra `ticket_no`, `ticket_label`), SalesClosed 403, SoldOut 409, PresaleClosedForever 409, NotFound 404, Conflict 409 (code `conflict`), Unauthorized 401, Internal 500 (log error, ข้อความทั่วไป)
    - contact: `pub fn set_contact_info(s: String)` เก็บใน `static OnceLock<String>`; main เรียกตอน start
  - `impl From<sqlx::Error> for AppError` → Internal
  - `tests/common/mod.rs`: `pub fn test_config() -> Config` (total 600, slip_dir = tempdir, password `"admin-pass"`, secret `"test-secret"`, contact `"LINE @hticket"`), `pub fn app(pool: PgPool, config: Config) -> Router`, `pub async fn send(app: &Router, req: Request<Body>) -> (StatusCode, serde_json::Value)`

Migration (ตามสเปกข้อ 4):
```sql
CREATE TABLE orders (
  id uuid PRIMARY KEY,
  ticket_no smallint NOT NULL,
  status text NOT NULL CHECK (status IN ('active','cancelled')),
  name text NOT NULL, student_id text NOT NULL, phone text NOT NULL,
  slip_path text NOT NULL, slip_ref text NOT NULL UNIQUE,
  created_at timestamptz NOT NULL DEFAULT now(),
  cancelled_at timestamptz, cancel_reason text, picked_up_at timestamptz
);
CREATE UNIQUE INDEX orders_active_ticket ON orders (ticket_no) WHERE status = 'active';
CREATE UNIQUE INDEX orders_active_student ON orders (student_id) WHERE status = 'active';
CREATE TABLE settings (key text PRIMARY KEY, value text NOT NULL);
INSERT INTO settings VALUES ('sales_open','false'), ('presale_closed_forever','false');
CREATE TABLE rejected_attempts (
  id uuid PRIMARY KEY, reason text NOT NULL,
  name text, student_id text, phone text, slip_path text, slip_ref text,
  created_at timestamptz NOT NULL DEFAULT now()
);
```
(`ticket_no` NOT NULL: เราออกเลขพร้อม insert เสมอ — ต่างจากสเปกเล็กน้อยเพราะไม่มีสถานะ "รอเลข")

`docker-compose.yml` service `db`: `postgres:16-alpine`, user/pass/db = `hticket`, port `5432:5432`, volume `pgdata`, healthcheck `pg_isready -U hticket`

- [ ] **Step 1:** เขียน test `healthz_returns_ok` ใน `tests/public_api.rs` (`#[sqlx::test(migrations = "./migrations")]`): GET `/healthz` → 200, body `{"status":"ok"}`
- [ ] **Step 2:** `docker compose up -d db && cargo test --test public_api` → FAIL (compile error)
- [ ] **Step 3:** implement ไฟล์ข้างบน + route `GET /healthz`; `main.rs`: init tracing, `Config::from_env`, `set_contact_info`, `PgPoolOptions::new().max_connections(10)`, `sqlx::migrate!().run`, `create_dir_all(slip_dir)`, bind `0.0.0.0:{port}`
- [ ] **Step 4:** `cargo test --test public_api` → PASS; `cargo clippy -- -D warnings` สะอาด
- [ ] **Step 5:** commit `feat: scaffold axum app, config, migrations, db compose`

---

### Task 2: ตรวจสลิปจาก QR

**Files:**
- Create: `src/slip.rs` (unit tests ในไฟล์)

**Interfaces:**
- Produces:
  - `#[derive(Debug, PartialEq)] pub enum SlipError { NoQr, NotSlip }`
  - `pub fn parse_slip_payload(payload: &str) -> Result<String, SlipError>` — คืน transaction ref
  - `pub fn read_slip_ref(image_bytes: &[u8]) -> Result<String, SlipError>` — decode รูป → หา QR ทุกอันใน `rqrr::PreparedImage` → อันแรกที่ `parse_slip_payload` ผ่าน
  - `pub fn encode_slip_payload(trans_ref: &str) -> String` — สร้าง payload ที่ถูกต้อง (ใช้ใน test ทุกที่)
  - `fn crc16_ccitt(data: &[u8]) -> u16` (poly 0x1021, init 0xFFFF, ไม่ reflect)

เกณฑ์ `parse_slip_payload` (สเปกข้อ 3): parse TLV ระดับบนสุดได้ทั้งสาย (tag 2 หลัก, length 2 หลักฐาน 10, value ยาวตามนั้น ไม่มีเศษ); tag `00` มีอยู่และ parse TLV ย่อยได้ และมี sub-tag `02` ไม่ว่าง; tag `51` == `"TH"`; tag `91` เป็นตัวสุดท้าย ยาว 4 เป็น hex และเท่ากับ `crc16_ccitt(payload ก่อนค่า CRC รวม "9104")` (เทียบแบบไม่สนตัวพิมพ์)

`encode_slip_payload(r)`: `sub = "0006000001" + "0103014" + "02" + len2(r) + r`; `body = "00" + len2(sub) + sub + "5102TH" + "9104"`; คืน `body + format!("{:04X}", crc16_ccitt(body))`

- [ ] **Step 1:** unit tests:
  - `crc16_known_vector`: `crc16_ccitt(b"123456789") == 0x29B1`
  - `roundtrip`: `parse_slip_payload(&encode_slip_payload("015123100512ABC")) == Ok("015123100512ABC")`
  - `bad_crc_rejected`: เปลี่ยนตัวอักษรสุดท้ายของ payload → `Err(NotSlip)`
  - `promptpay_payment_qr_rejected`: `"00020101021129370016A000000677010111011300668123456785802TH530376463043B5F"` (QR รับเงิน ไม่ใช่สลิป) → `Err(NotSlip)`
  - `url_qr_rejected`: `"https://example.com"` → `Err(NotSlip)`
  - `image_without_qr`: PNG สีขาว 200×200 → `Err(NoQr)`; bytes ขยะ → `Err(NoQr)`
  - `image_with_slip_qr`: render `encode_slip_payload("REF1")` ด้วย `qrcode` เป็น PNG → `Ok("REF1")`
  - `small_qr_in_large_photo` (Review Focus 4): วาง QR ขนาด ~250px ลงบน canvas สีเทาอ่อน 2000×3000 ที่ตำแหน่ง (1500, 2500) → `Ok`
- [ ] **Step 2:** `cargo test slip::` → FAIL
- [ ] **Step 3:** implement; ถ้า `small_qr_in_large_photo` อ่านไม่ได้ ให้ลอง decode ซ้ำบนรูปที่ย่อด้านยาวเหลือ 1600px
- [ ] **Step 4:** `cargo test slip::` → PASS
- [ ] **Step 5:** commit `feat: verify Thai bank slip mini-QR`

---

### Task 3: Validation

**Files:**
- Create: `src/validate.rs` (unit tests ในไฟล์)

**Interfaces:**
- Consumes: `AppError::InvalidInput`
- Produces:
  - `#[derive(Debug, Clone)] pub struct NewOrderInput { pub name: String, pub student_id: String, pub phone: String }`
  - `pub fn validate_order(name: &str, student_id: &str, phone: &str) -> Result<NewOrderInput, AppError>` — trim ทุกช่องก่อนตรวจ
  - `pub fn normalize_student_id(s: &str) -> Option<String>` — trim, ต้องเป็นตัวเลข 10 หลัก
  - `#[derive(Debug, Clone, Copy, PartialEq)] pub enum ImageKind { Jpeg, Png, Webp }` + `pub fn ext(self) -> &'static str` (`jpg`/`png`/`webp`) + `pub fn mime(self) -> &'static str`
  - `pub fn detect_image(bytes: &[u8]) -> Result<ImageKind, AppError>` — magic bytes; ว่าง/ไม่รู้จัก → InvalidInput; `> 5 * 1024 * 1024` → InvalidInput("ไฟล์ใหญ่เกิน 5 MB")

- [ ] **Step 1:** tests: ผ่าน `("  สมชาย ใจดี ", " 6831501178 ", "0812345678")` แล้วได้ค่าที่ trim แล้ว; ปฏิเสธ ชื่อว่าง, ชื่อ 101 ตัวอักษรไทย, รหัส 9 หลัก/มีตัวอักษร, เบอร์ 9 หลัก/ขึ้นต้น `1`; `detect_image` ถูกกับ header JPEG `FF D8 FF`, PNG `89 50 4E 47 0D 0A 1A 0A`, WebP `RIFF????WEBP`; ปฏิเสธ `%PDF` และ 5 MB + 1 byte
- [ ] **Step 2:** `cargo test validate::` → FAIL
- [ ] **Step 3:** implement
- [ ] **Step 4:** `cargo test validate::` → PASS
- [ ] **Step 5:** commit `feat: validate order input and slip image type`

---

### Task 4: DB layer — ออกเลขบัตร, settings, rejected attempts

**Files:**
- Create: `src/db/mod.rs`, `src/db/orders.rs`, `src/db/settings.rs`, `src/db/rejected.rs`
- Test: `tests/issue.rs`

**Interfaces:**
- Consumes: `NewOrderInput`, `AppError`
- Produces:
  - `#[derive(Debug, Clone, Serialize, sqlx::FromRow)] pub struct Order { id: Uuid, ticket_no: i16, status: String, name, student_id, phone, slip_path: String, slip_ref: String, created_at: DateTime<Utc>, cancelled_at: Option<DateTime<Utc>>, cancel_reason: Option<String>, picked_up_at: Option<DateTime<Utc>> }` (ทุก field `pub`)
  - `orders::issue_ticket(pool: &PgPool, total: i16, order_id: Uuid, input: &NewOrderInput, slip_ref: &str, slip_path: &str) -> Result<Order, AppError>`
  - `orders::find_active_by_student(pool, student_id: &str) -> Result<Option<Order>, AppError>`
  - `orders::get_order(pool, id: Uuid) -> Result<Order, AppError>` (ไม่มี → NotFound)
  - `orders::list_orders(pool, q: Option<&str>, status: Option<&str>) -> Result<Vec<Order>, AppError>` — `q` ค้น ILIKE ใน name/student_id/phone หรือเท่ากับ ticket_no ถ้าเป็นตัวเลข; เรียงตาม ticket_no, created_at
  - `orders::cancel_order(pool, id, reason: &str) -> Result<Order, AppError>` — ยกเลิกแล้ว → `Conflict("บัตรนี้ถูกยกเลิกไปแล้ว")`
  - `orders::set_picked_up(pool, id, picked_up: bool) -> Result<Order, AppError>` — ใบที่ยกเลิกแล้ว → Conflict
  - `#[derive(Serialize)] pub struct Stats { sold: i64, remaining: i64, cancelled: i64, picked_up: i64, free_numbers: Vec<i16> }`; `orders::stats(pool, total) -> Result<Stats, AppError>` (`free_numbers` = 1..=total ที่ไม่มี active)
  - `#[derive(Serialize, Clone, Copy)] pub struct SalesState { open: bool, closed_forever: bool }`; `settings::get_sales_state(pool)`, `settings::set_sales_open(pool, open: bool) -> Result<SalesState, AppError>` (closed_forever && open → PresaleClosedForever), `settings::close_forever(pool) -> Result<SalesState, AppError>` (ตั้งทั้งสองค่าใน transaction เดียว)
  - `#[derive(Serialize, FromRow)] pub struct RejectedAttempt { id, reason, name, student_id, phone, slip_path, slip_ref: Option<String>…, created_at }`; `rejected::insert(pool, reason: &str, name: Option<&str>, student_id: Option<&str>, phone: Option<&str>, slip_path: Option<&str>, slip_ref: Option<&str>) -> Result<Uuid, AppError>`; `rejected::list(pool)` (ใหม่สุดก่อน); `rejected::get(pool, id)`

`issue_ticket` ใน transaction เดียว ตามลำดับนี้:
1. `SELECT pg_advisory_xact_lock(7001)`
2. อ่าน sales state ใน tx → ไม่ open → SalesClosed
3. มี active ของ student → `AlreadyHasTicket { ticket_no }`
4. slip_ref มีใน orders (ทุกสถานะ) → DuplicateSlip
5. `SELECT min(g)::smallint FROM generate_series(1, $1::int) g WHERE NOT EXISTS (SELECT 1 FROM orders WHERE status='active' AND ticket_no=g)` → NULL → SoldOut
6. INSERT, commit
ถ้า INSERT ชน unique violation (ไม่ควรเกิด) map เป็น Internal พร้อม log

- [ ] **Step 1:** tests ใน `tests/issue.rs` (ทุกตัว `#[sqlx::test]`, เปิดขายด้วย `set_sales_open(true)` ก่อน; helper สร้าง input รหัส `68000000NN`):
  - `first_ticket_is_1`
  - `concurrent_50_distinct_numbers`: spawn 50 task พร้อมกัน → เลข sorted == 1..=50
  - `same_student_concurrent_one_ticket` (Review Focus 1): รหัสเดียว 2 task สลิปต่างกัน → Ok 1 ตัว, `AlreadyHasTicket` 1 ตัว
  - `cancelled_number_reused`: ออก 1..5, ยกเลิก 3 → ใบถัดไปได้ 3
  - `duplicate_slip_rejected_even_after_cancel`: ออก ref "A", ยกเลิก, ส่ง "A" รหัสอื่น → DuplicateSlip
  - `student_can_rebuy_after_cancel`
  - `sold_out_when_total_reached`: total=3 ออก 3 ใบ → ใบที่ 4 SoldOut
  - `sales_closed_rejected`
  - `close_forever_blocks_reopen`: close_forever → `set_sales_open(true)` = PresaleClosedForever, state `{open:false, closed_forever:true}`
  - `stats_free_numbers`: total=5 ออก 1..3 ยกเลิก 2 → sold 2, cancelled 1, free `[2,4,5]`
- [ ] **Step 2:** `cargo test --test issue` → FAIL
- [ ] **Step 3:** implement
- [ ] **Step 4:** `cargo test --test issue` → PASS (รัน 3 รอบเพื่อตรวจว่า concurrency test ไม่ flaky)
- [ ] **Step 5:** commit `feat: race-free ticket number allocation`

---

### Task 5: Slip storage + public API

**Files:**
- Create: `src/storage.rs`, `src/routes/mod.rs`, `src/routes/public.rs`
- Modify: `src/lib.rs` (mount routes, `DefaultBodyLimit::max(6 * 1024 * 1024)` บน `/api/orders`)
- Modify: `tests/common/mod.rs` (`pub fn slip_png(trans_ref: &str) -> Vec<u8>`, `pub fn multipart_order(name, student_id, phone, file: &[u8]) -> Request<Body>` boundary คงที่, `pub async fn open_sales(pool)`)
- Test: `tests/public_api.rs`

**Interfaces:**
- Consumes: Task 2–4 ทั้งหมด
- Produces:
  - `#[derive(Clone)] pub struct SlipStore { dir: PathBuf }`; `SlipStore::new(dir)`; `async fn save_temp(&self, bytes: &[u8]) -> io::Result<PathBuf>` (ชื่อ `tmp-<uuid>`); `async fn finalize(&self, temp: &Path, name: &str) -> io::Result<String>` (rename → คืน relative path `orders/<name>`); `async fn save_rejected(&self, bytes, name) -> io::Result<String>` (`rejected/<name>`); `async fn read(&self, rel: &str) -> io::Result<Vec<u8>>` (ปฏิเสธ rel ที่มี `..`)
  - `pub fn ticket_view(o: &Order) -> serde_json::Value` = `{ticket_no, ticket_label, name, student_id, picked_up: bool}` (ไม่มี phone) — Task 6 ไม่ใช้

Routes:
- `GET /api/sales` → `{open, closed_forever, total, sold, remaining}`
- `POST /api/orders` multipart (`name`, `student_id`, `phone`, `slip`) ลำดับ:
  1. อ่านฟิลด์; ขาดฟิลด์ → InvalidInput; `validate_order`; `detect_image(slip)`
  2. sales state ไม่ open → SalesClosed (ไม่บันทึก)
  3. `read_slip_ref` (ใน `spawn_blocking`) ล้ม → บันทึก rejected (`invalid_slip`, รูปที่ `rejected/<id>.<ext>`) → InvalidSlip
  4. `save_temp` → `issue_ticket(order_id, slip_path = "orders/<order_id>.<ext>")` → สำเร็จ `finalize` → `201 ticket_view`
  5. ล้มด้วย DuplicateSlip/AlreadyHasTicket → ย้าย temp ไป rejected + บันทึก rejected; error อื่น → ลบ temp; คืน error เดิม
- `GET /api/tickets/{student_id}` → `normalize_student_id` (ไม่ผ่าน → InvalidInput) → active → 200 `ticket_view`, ไม่มี → NotFound

- [ ] **Step 1:** tests:
  - `post_order_issues_ticket`: 201, `ticket_label == "001"`, ไม่มี key `phone`, มีไฟล์ใน `slip_dir/orders/`
  - `lookup_ticket_with_spaces` (Review Focus 2): GET `/api/tickets/%206831501178%20` → 200 เลขเดิม
  - `lookup_unknown_404`
  - `non_slip_image_rejected_and_logged`: PNG ที่มี QR `"https://example.com"` → 422 `invalid_slip`, `contact == "LINE @hticket"`, message มี "กรุณาติดต่อทีมงาน", `rejected::list` มี 1 แถวที่ slip_path ไม่ว่าง
  - `duplicate_slip_409_and_logged`
  - `already_has_ticket_returns_existing_number`: body มี `ticket_label: "001"`
  - `sales_closed_403_not_logged`
  - `oversized_upload_rejected` (Review Focus 3): ไฟล์ 7 MB → status 400 หรือ 413 และ body เป็น JSON ที่มี `code` (แปลง rejection ของ multipart/body limit เป็น `AppError::InvalidInput("ไฟล์ใหญ่เกิน 5 MB")`)
  - `sales_endpoint_counts`
- [ ] **Step 2:** `cargo test --test public_api` → FAIL
- [ ] **Step 3:** implement
- [ ] **Step 4:** `cargo test` → PASS ทั้งหมด
- [ ] **Step 5:** commit `feat: public order and ticket lookup API`

---

### Task 6: Admin auth + admin API

**Files:**
- Create: `src/auth.rs`, `src/routes/admin.rs`
- Modify: `src/lib.rs`, `src/routes/mod.rs`
- Test: `tests/admin_api.rs`

**Interfaces:**
- Consumes: Task 4 db fns, `SlipStore::read`
- Produces:
  - `pub fn issue_token(secret: &str) -> String` (claims `{sub:"admin", exp: now+12h}`), `pub fn verify_token(secret: &str, token: &str) -> bool`
  - `pub struct AdminAuth;` implement `FromRequestParts<AppState>` อ่าน `Authorization: Bearer` → ไม่ผ่าน `AppError::Unauthorized`
  - เทียบรหัสผ่านแบบ constant-time (เทียบ SHA-256 ของทั้งสองฝั่ง หรือ `subtle`)

Routes (ตามตาราง API ในสเปก):
- `POST /api/admin/login` `{password}` → `{token}` / 401
- `GET /api/admin/stats` → `Stats` + `open`, `closed_forever`, `total`
- `GET /api/admin/orders?q=&status=` → `Order[]` (มี phone, `ticket_label`)
- `GET /api/admin/orders/{id}/slip` → bytes + `Content-Type` จากนามสกุล
- `POST /api/admin/orders/{id}/cancel` `{reason}` (reason ว่าง → InvalidInput) → Order
- `POST /api/admin/orders/{id}/pickup` `{picked_up}` → Order
- `PUT /api/admin/sales` `{open}` → SalesState
- `POST /api/admin/sales/close-forever` → SalesState
- `GET /api/admin/rejected`, `GET /api/admin/rejected/{id}/slip` (slip_path ว่าง → NotFound)
- path `{id}` ที่ไม่ใช่ UUID → 400 JSON (ไม่ใช่ข้อความ plain ของ axum)

- [ ] **Step 1:** tests:
  - `login_wrong_password_401`, `login_ok_returns_token`
  - `admin_routes_require_token`: ทุก route ข้างบนที่ไม่มี token / token ผิด → 401
  - `expired_token_401`: สร้าง token ที่ exp อยู่ในอดีตด้วย `jsonwebtoken` ตรง ๆ
  - `cancel_then_lookup_404_and_number_free`
  - `cancel_twice_409` (Review Focus 5), `cancel_unknown_id_404`, `cancel_bad_uuid_400_json`
  - `pickup_toggle`: true → `/api/tickets/{sid}` แสดง `picked_up: true`
  - `slip_download_matches_upload`: bytes เท่ากับที่อัปโหลด, content-type `image/png`
  - `search_by_ticket_no_and_name`
  - `close_forever_then_reopen_409`
  - `rejected_list_and_slip`
- [ ] **Step 2:** `cargo test --test admin_api` → FAIL
- [ ] **Step 3:** implement
- [ ] **Step 4:** `cargo test` → PASS ทั้งหมด; `cargo clippy --all-targets -- -D warnings` สะอาด
- [ ] **Step 5:** commit `feat: admin API with JWT auth`

---

### Task 7: Docker image, full compose, README

**Files:**
- Create: `Dockerfile`, `.dockerignore` (`target`, `.git`, `.env`, `data`)
- Modify: `docker-compose.yml` (เพิ่ม `backend`: build `.`, `env_file: .env`, `DATABASE_URL=postgres://hticket:hticket@db:5432/hticket`, `SLIP_DIR=/data/slips`, volume `slips:/data/slips`, port `8080:8080`, `depends_on: db: condition: service_healthy`, `restart: unless-stopped`), `.env.example` (ทุก env พร้อมคำอธิบายภาษาไทย), `README.md` (ภาษาไทย: ติดตั้ง Docker, `cp .env.example .env`, `docker compose up -d --build`, รัน test, ตาราง API สั้น ๆ, วิธีย้ายไปเซิร์ฟเวอร์ 100.109.2.36 ด้วย `git clone` + compose เดียวกัน)

Dockerfile: stage 1 `rust:1-slim-bookworm` → `cargo build --release` (cache deps ด้วย dummy `main.rs` ก่อน copy src); stage 2 `debian:bookworm-slim` + `ca-certificates`, copy binary + `migrations/` ไม่จำเป็น (ฝังด้วย `migrate!`), รันด้วย user ไม่ใช่ root, `EXPOSE 8080`

- [ ] **Step 1:** `cp .env.example .env` (ตั้งรหัสทดสอบ) → `docker compose up -d --build`
- [ ] **Step 2:** smoke test ด้วย curl → ต้องได้ตามนี้:
  - `curl localhost:8080/healthz` → `{"status":"ok"}`
  - login → token; `PUT /api/admin/sales {"open":true}` → `{"open":true,...}`
  - POST `/api/orders` ด้วย PNG QR สลิปที่สร้างจาก `cargo test` helper (บันทึกไฟล์ไว้ที่ `tests/fixtures/sample-slip.png`, commit ได้เพราะเป็นข้อมูลปลอม) → 201 `"ticket_label":"001"`
  - `docker compose restart backend` แล้ว GET `/api/tickets/<sid>` ยังได้ 001 และ GET slip ยังได้ (volume ทำงาน)
- [ ] **Step 3:** `docker compose down` (ไม่ใส่ `-v`)
- [ ] **Step 4:** commit `chore: dockerize backend and document local run`
