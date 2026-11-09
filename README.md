# H-Ticket Backend

Backend ระบบขายบัตร pre-sale บ้านผีสิง (Rust + Axum + PostgreSQL)
ผู้ซื้อกรอกข้อมูล + แนบสลิป → ระบบอ่าน QR บนสลิป → ออกเลขบัตร 001–600 ให้ทันที

เอกสารออกแบบ: `docs/superpowers/specs/2026-11-07-ticket-issuing-design.md`

## รันบนเครื่อง (Docker)

ต้องมี Docker (บน Mac แนะนำ OrbStack)

```bash
cp .env.example .env        # แล้วแก้ ADMIN_PASSWORD (≥12 ตัว), JWT_SECRET (≥32 ตัว), CONTACT_INFO
docker compose up -d --build
curl localhost:8080/healthz # {"status":"ok"}
```

- ข้อมูลฐานข้อมูลอยู่ใน volume `pgdata`, รูปสลิปอยู่ใน volume `slips`
- หยุด: `docker compose down` (**อย่า** ใส่ `-v` ไม่งั้นข้อมูลหาย)
- การขายเริ่มต้นเป็น "ปิด" ต้องให้แอดมินเปิดก่อน (`PUT /api/admin/sales`)

## รันเทสต์

```bash
docker compose up -d db
cargo test
```

`DATABASE_URL` สำหรับ dev ตั้งไว้ใน `.cargo/config.toml` (รหัส `hticket`) ถ้าตั้ง `POSTGRES_PASSWORD` ใน `.env` เป็นค่าอื่น ให้ export เองก่อนรันเทสต์:
```bash
export DATABASE_URL="postgres://hticket:<POSTGRES_PASSWORD>@localhost:5432/hticket"
```

สร้างรูปสลิปปลอมไว้ลองยิง API ด้วยมือ:
```bash
cargo run --example make_sample_slip -- REF123 /tmp/slip.png
```
(มีตัวอย่างพร้อมใช้ที่ `tests/fixtures/sample-slip.png`)

## API

ทุก error คืน `{ "code": "...", "message": "ข้อความภาษาไทย" }`

### สาธารณะ
| Method | Path | |
|---|---|---|
| GET | `/api/sales` | `{open, closed_forever, total, sold, remaining}` |
| POST | `/api/orders` | multipart: `name`, `student_id` (ส่งซ้ำได้ 1, 2 หรือ 5 คู่ = โปรกลุ่ม), `phone` (ของผู้ซื้อ), `slip` → 201 `{ticket_no, ticket_label, name, student_id, picked_up, group_size, tickets: [...]}` (ตัวนอกคือบัตรคนแรก) |
| GET | `/api/tickets/{student_id}` | บัตรของรหัสนี้ หรือ 404 (ถ้าซื้อเป็นกลุ่มมี `group: [{ticket_label, name}]`) |
| GET | `/healthz` | health check |

error ของ `POST /api/orders`: `invalid_input` 400, `sales_closed` 403, `invalid_slip` 422,
`duplicate_slip` 409, `already_has_ticket` 409 (+`ticket_label`, `student_id` ของคนที่มีบัตรแล้ว), `sold_out` 409 (บัตรเหลือไม่พอทั้งกลุ่ม)
กลุ่มออกบัตรแบบได้ทั้งหมดหรือไม่ได้เลย สลิปเดียวใช้ได้แค่กลุ่มเดียว ระบบไม่ตรวจยอดเงิน แอดมินดู `group_size` เทียบยอดบนสลิปเอง
(`invalid_slip`/`duplicate_slip` มี `contact` ให้แสดงช่องทางติดต่อทีมงาน)

### แอดมิน (header `Authorization: Bearer <token>`)
| Method | Path | |
|---|---|---|
| POST | `/api/admin/login` | `{password}` → `{token}` (อายุ 12 ชม.) — ผิดครบ 5 ครั้งใน 1 นาที → 429 `too_many_attempts` ต้องรอ |
| GET | `/api/admin/stats` | ขายแล้ว/เหลือ/ยกเลิก/รับบัตรแล้ว + `free_numbers` |
| GET | `/api/admin/orders?q=&status=` | รายชื่อ ค้นด้วยชื่อ/รหัส/เบอร์/เลขบัตร, `status=active\|cancelled` |
| GET | `/api/admin/orders/{id}/slip` | รูปสลิป |
| POST | `/api/admin/orders/{id}/cancel` | `{reason}` ยกเลิกบัตร เลขกลับมาว่าง |
| POST | `/api/admin/orders/{id}/pickup` | `{picked_up: true/false}` |
| PUT | `/api/admin/sales` | `{open: true/false}` |
| POST | `/api/admin/sales/close-forever` | ปิด pre-sale ถาวร (ย้อนไม่ได้) |
| GET | `/api/admin/rejected` | ความพยายามซื้อที่ถูกปฏิเสธ (สลิปไม่ผ่าน/ซ้ำ/มีบัตรแล้ว) |
| GET | `/api/admin/rejected/{id}/slip` | รูปของความพยายามนั้น |

## ย้ายไปเซิร์ฟเวอร์ 100.109.2.36

```bash
git clone <repo> && cd H-Ticket-Backend
cp .env.example .env   # ตั้งรหัสจริง + CORS_ORIGIN เป็น URL ของ frontend
docker compose up -d --build
```

Postgres เปิดพอร์ตเฉพาะ `127.0.0.1` (เครื่องอื่นเข้าไม่ได้) — บนเซิร์ฟเวอร์ตั้ง `POSTGRES_PASSWORD` ใน `.env` เป็นค่ายาว ๆ **ก่อน** `docker compose up` ครั้งแรก
การเปิดให้คนนอก Tailscale เข้าได้ (Cloudflare Tunnel / Tailscale Funnel) ยังไม่ได้ตั้งค่า
