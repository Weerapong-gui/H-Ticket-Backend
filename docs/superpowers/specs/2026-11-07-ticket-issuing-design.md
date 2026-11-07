# ระบบออกบัตร Pre-sale บ้านผีสิง — Backend Design

วันที่: 2026-11-07
ขอบเขต: เฉพาะ backend (Rust) ของระบบออกบัตร ส่วน frontend (Next.js) ทำแยกทีหลัง

## 1. เป้าหมายและเหตุผล

ปกติบ้านผีสิงขายบัตรหน้างาน บัตรจริงเป็นกระดาษพิมพ์เลขเรียง 001–600 ปีนี้อยากเปิด **pre-sale ผ่านเว็บก่อนวันงาน**
คาดว่าคนซื้อผ่านเว็บไม่เยอะ ส่วนที่เหลือยังขายหน้างานตามเดิม

เลขบัตรในเว็บ = เลขบัตรจริง = ลำดับคิวเข้าบ้านผีสิง คนที่จองเว็บได้เลข 001 จะได้รับบัตรจริงใบ 001

### Flow ฝั่งผู้ซื้อ
1. กรอก ชื่อ, รหัสนักศึกษา, เบอร์โทร
2. โอนเงิน แล้วแนบรูปสลิป
3. ระบบตรวจว่าสลิปเป็นสลิปธนาคารจริงและไม่เคยใช้มาก่อน → ออกเลขบัตรให้รหัสนักศึกษานั้น **ทันที** (ไม่มีคนตรวจก่อน)
4. วันรับบัตร: เข้าเว็บ พิมพ์รหัสนักศึกษา → ระบบแสดงบัตรพร้อมเลขคิว → โชว์ทีมงานเพื่อรับบัตรจริงเลขเดียวกัน

### Flow ฝั่งแอดมิน
- แดชบอร์ด: ขายแล้วกี่ใบ เหลือกี่ใบ เลขที่ว่างอยู่มีอะไรบ้าง
- รายชื่อผู้ซื้อทั้งหมด ค้นหาได้ เปิดดูสลิปรายคนได้
- ถ้าเจอสลิปปลอม/ผิดปกติ → **ยกเลิกบัตร** เลขนั้นกลับมาว่าง
- เปิด/ปิดการขายผ่านเว็บ
- ติ๊ก "รับบัตรจริงแล้ว" ตอนแจกบัตร

### การแบ่งเลขกับการขายหน้างาน
งานเปิด **วันเดียว** มีคิวเดียว 001–600 (ผู้ใช้ยืนยัน)

ก่อนวันงาน แอดมินกด **"ปิด pre-sale ถาวร"** (ผู้ใช้ตกลง) หลังจากกดแล้ว เปิดขายผ่านเว็บอีกไม่ได้ ไม่ว่ากรณีใด
แอดมินดูรายการเลขว่าง แล้วทีมหน้างานขายบัตรจริงเฉพาะเลขที่ว่าง
(ส่วนใหญ่คือต่อจากเลขสุดท้ายของเว็บ บวกเลขที่ถูกยกเลิก) ทีมหน้างานไม่ต้องใช้ระบบ
ถ้ายกเลิกบัตรหลังปิดถาวร เลขนั้นจะไปโผล่ในรายการเลขว่างให้หน้างานขาย เว็บไม่มีทางออกเลขนั้นอีก

### สลิปจริงที่ระบบอ่านไม่ผ่าน (ผู้ใช้ตัดสินใจ)
ปฏิเสธไปเลย และในข้อความแจ้งผู้ซื้อให้ติดต่อทีมงาน (แนบช่องทางติดต่อจาก config `CONTACT_INFO`) ไม่มีสถานะ "รอตรวจ"

## 2. Goals / Non-goals / Anti-goals

**Goals**
- รับคำสั่งซื้อ + สลิป, ตรวจสลิป, ออกเลขบัตรอัตโนมัติ
- ค้นหาบัตรด้วยรหัสนักศึกษา
- API แอดมิน: แดชบอร์ด, รายชื่อ, ดูสลิป, ยกเลิก, เปิด/ปิดขาย, ติ๊กรับบัตร
- รันบน Docker Compose ได้ทั้งบนเครื่อง dev (Mac) และเซิร์ฟเวอร์ 100.109.2.36

**Non-goals (รอบนี้ไม่ทำ)**
- ตรวจยอดเงิน และตรวจว่าโอนเข้าบัญชีถูก (ผู้ใช้ไม่กังวลเรื่องนี้) — เผื่อเพิ่ม SlipOK/EasySlip ทีหลัง
- บันทึกการขายหน้างานเข้าระบบ
- ส่ง SMS / อีเมล
- Frontend
- การเปิดเว็บสู่สาธารณะ (Cloudflare Tunnel / Tailscale Funnel / โดเมน) — ตัดสินใจตอน deploy

**Anti-goals (ถ้าเกิดถือว่าล้มเหลว แม้ระบบจะ "ทำงาน")**
- มีสองคนถือเลขบัตรเดียวกัน (active พร้อมกัน)
- รหัสนักศึกษาเดียวมีบัตร active มากกว่า 1 ใบ
- สลิปใบเดียวถูกใช้ออกบัตรได้มากกว่า 1 ครั้ง
- ออกบัตรเกินจำนวนบัตรทั้งหมด
- หลักฐาน (รูปสลิป, ข้อมูลผู้ซื้อ) หายไปเมื่อบัตรถูกยกเลิก

## 3. สิ่งที่ตัดสินใจแล้ว

### Stack (ผู้ใช้ตกลงตามที่เสนอ)
- Rust + **Axum** (tokio)
- **PostgreSQL** ผ่าน **sqlx** (migration ด้วย `sqlx::migrate!` ตอน start)
- รูปสลิปเก็บเป็นไฟล์ใน Docker volume
- **Docker Compose**: service `backend` + `db` (postgres)
- ตอนนี้รัน local บน Mac ก่อน ใช้ compose ชุดเดียวกับที่จะเอาไปลงเซิร์ฟเวอร์

### กฎการออกเลขบัตร (ผู้ใช้ตัดสินใจ)
- 1 รหัสนักศึกษา = บัตร active ได้ 1 ใบ
- เลขบัตร 1..TOTAL_TICKETS (ค่าเริ่ม 600) แสดงผลเป็น 3 หลัก (`001`)
- คนถัดไปได้ **เลขที่ว่างและน้อยที่สุด** (เลขที่ถูกยกเลิกจะถูกนำกลับมาใช้)
- ยกเลิกบัตร = เปลี่ยนสถานะเป็น cancelled (ไม่ลบแถว ไม่ลบรูปสลิป) เลขกลับมาว่าง
- ครบจำนวนแล้ว → ปฏิเสธคำสั่งซื้อใหม่ ("บัตรหมด")

### การตรวจสลิป (ผู้ใช้ตกลง: แค่ต้องเป็นสลิปจริง)
สลิปโอนเงินธนาคารไทยมี QR ("mini QR") ที่เข้ารหัสข้อมูลรูปแบบ EMVCo TLV
- decode QR จากรูป (เช่น crate `rqrr` + `image`)
- ไม่พบ QR / parse TLV ไม่ได้ / ไม่ใช่รูปแบบสลิป → ปฏิเสธ ("ไม่พบสลิปที่ถูกต้อง")
- เกณฑ์ว่าเป็นสลิป: TLV ถูกต้อง, มี tag `00` ที่มี sub-tag `02` (transaction ref), มี tag `51` = `TH`, CRC16 (tag `91`) ถูกต้อง
- เก็บ transaction ref เป็น `UNIQUE` ทั้งตาราง (รวมแถวที่ถูกยกเลิก) → สลิปซ้ำถูกปฏิเสธเสมอ
- ⚠️ builder ต้องทดสอบกับรูปสลิปจริงจากหลายธนาคารก่อนสรุปเกณฑ์ (ขอสลิปตัวอย่างจากผู้ใช้)

## 4. ข้อเสนอโครงสร้าง (คำตัดสินของผู้ออกแบบ — builder ปรับได้ถ้ามีเหตุผล)

### ตาราง
```
orders
  id            uuid PK
  ticket_no     smallint NULL       -- เลขบัตร
  status        text  ('active' | 'cancelled')
  name          text
  student_id    text
  phone         text
  slip_path     text                -- path ไฟล์ใน volume
  slip_ref      text UNIQUE         -- transaction ref จาก QR
  created_at    timestamptz
  cancelled_at  timestamptz NULL
  cancel_reason text NULL
  picked_up_at  timestamptz NULL    -- รับบัตรจริงแล้ว

  UNIQUE (ticket_no)  WHERE status = 'active'
  UNIQUE (student_id) WHERE status = 'active'

settings
  key text PK, value text
  -- sales_open            = 'true' | 'false'
  -- presale_closed_forever = 'true' | 'false'  (ตั้งเป็น true แล้วห้ามกลับ, ถ้า true → sales_open บังคับ false)

rejected_attempts                   -- (คำตัดสินของผู้ออกแบบ) เก็บความพยายามที่ถูกปฏิเสธไว้ให้แอดมินดู
  id            uuid PK
  reason        text                -- error code เช่น duplicate_slip, invalid_slip, already_has_ticket
  name, student_id, phone  text NULL
  slip_path     text NULL           -- เก็บรูปถ้าอัปโหลดมาเป็นรูปที่ถูกประเภท
  slip_ref      text NULL           -- ถ้าอ่าน QR ได้
  created_at    timestamptz
```
ไม่บันทึก attempt ที่ล้มเพราะ `sales_closed`, `sold_out`, `invalid_input` (ไม่มีประโยชน์ด้านตรวจโกง)
Partial unique index เป็นด่านสุดท้ายกัน anti-goal แม้โค้ดมีบั๊ก

### การออกเลขแบบไม่ชน
ใน transaction เดียว:
1. `pg_advisory_xact_lock(<const>)` ให้ออกเลขทีละคำสั่ง
2. เช็ค sales_open, เช็คว่ารหัสนี้ไม่มีบัตร active, เช็ค slip_ref ไม่ซ้ำ
3. หาเลขว่างน้อยสุด: `SELECT min(g) FROM generate_series(1, $TOTAL) g WHERE g NOT IN (SELECT ticket_no FROM orders WHERE status='active')`
4. INSERT แล้ว commit
ต้องมี test ยิงพร้อมกันหลายคำสั่ง (เช่น 50 concurrent) แล้วตรวจว่าไม่มีเลขซ้ำ

ไฟล์สลิป: เขียนลง temp ก่อน แล้ว rename เป็นชื่อ `<order_id>.<ext>` หลัง commit สำเร็จ (ถ้า transaction ล้ม ลบ temp)

### Validation
- ชื่อ: ไม่ว่าง, ≤ 100 ตัวอักษร
- รหัสนักศึกษา: ตัวเลข 10 หลัก (รูปแบบ มฟล.)
- เบอร์โทร: ตัวเลข 10 หลัก ขึ้นต้นด้วย 0
- รูป: JPEG/PNG/WebP (ตรวจจาก magic bytes), ≤ 5 MB

### API (JSON, ข้อความ error ภาษาไทย + `code` ภาษาอังกฤษให้ frontend ใช้)

สาธารณะ
| Method | Path | ใช้ทำอะไร |
|---|---|---|
| GET | `/api/sales` | `{ open, total, sold, remaining }` |
| POST | `/api/orders` | multipart: `name`, `student_id`, `phone`, `slip` → `201 { ticket_no, name, student_id }` |
| GET | `/api/tickets/{student_id}` | บัตร active ของรหัสนี้ `{ ticket_no, name, picked_up }` หรือ 404 |
| GET | `/healthz` | health check |

error codes ของ POST `/api/orders`: `sales_closed`, `sold_out`, `already_has_ticket` (แนบเลขเดิม), `invalid_slip`, `duplicate_slip`, `invalid_input`

`/api/tickets/{student_id}` ไม่คืนเบอร์โทร (ใครพิมพ์รหัสก็ดูได้)

แอดมิน (ต้องมี `Authorization: Bearer <token>`)
| Method | Path | ใช้ทำอะไร |
|---|---|---|
| POST | `/api/admin/login` | `{ password }` → `{ token }` (JWT HS256, หมดอายุ 12 ชม.) |
| GET | `/api/admin/stats` | sold, remaining, cancelled, picked_up, sales_open, free_numbers[] |
| GET | `/api/admin/orders?q=&status=` | รายชื่อ ค้นด้วยชื่อ/รหัส/เบอร์/เลขบัตร |
| GET | `/api/admin/orders/{id}/slip` | ไฟล์รูปสลิป |
| POST | `/api/admin/orders/{id}/cancel` | `{ reason }` |
| POST | `/api/admin/orders/{id}/pickup` | `{ picked_up: bool }` |
| PUT | `/api/admin/sales` | `{ open: bool }` — ถ้าปิดถาวรแล้ว ส่ง `open: true` → 409 `presale_closed_forever` |
| POST | `/api/admin/sales/close-forever` | ปิด pre-sale ถาวร (ย้อนไม่ได้) |
| GET | `/api/admin/rejected` | รายการความพยายามที่ถูกปฏิเสธ |
| GET | `/api/admin/rejected/{id}/slip` | รูปสลิปของความพยายามนั้น |

`/api/sales` และ `/api/admin/stats` คืน `closed_forever` ด้วย
error ของ `invalid_slip` / `duplicate_slip` มีข้อความ "กรุณาติดต่อทีมงาน" + `contact` จาก `CONTACT_INFO`

### Config (env)
`DATABASE_URL`, `ADMIN_PASSWORD`, `JWT_SECRET`, `TOTAL_TICKETS=600`, `SLIP_DIR=/data/slips`, `CORS_ORIGIN` (URL ของ frontend), `PORT=8080`, `CONTACT_INFO` (เช่น LINE/เบอร์ทีมงาน)

### Docker
- Dockerfile แบบ multi-stage (build ด้วย rust image → runtime แบบ slim)
- compose: `db` (postgres:16, volume `pgdata`), `backend` (volume `slips`, depends_on db healthy)
- ค่าลับอยู่ใน `.env` (ไม่ commit) มี `.env.example`

## 5. สิ่งที่ปล่อยให้ builder ตัดสินใจ
- โครงสร้างโมดูลใน `src/`, crate สำหรับ error handling / logging
- รายละเอียดการ parse TLV ของ QR (ภายใต้เกณฑ์ข้อ 3)
- รูปแบบ pagination ของรายชื่อแอดมิน (จำนวนน้อย ไม่ทำก็ได้)
- Rate limiting (ไม่จำเป็นรอบนี้)

## 6. การทดสอบที่ต้องมี
- ออกเลขพร้อมกันหลายคำสั่ง → ไม่ซ้ำ, ได้ 1..N ต่อเนื่อง
- ยกเลิกเลข 12 → คำสั่งถัดไปได้ 12
- รหัสซ้ำ, สลิปซ้ำ (รวมสลิปของบัตรที่ถูกยกเลิก), ปิดขาย, บัตรหมด → error code ถูกต้อง
- รูปที่ไม่มี QR / QR ที่ไม่ใช่สลิป → `invalid_slip`
- API แอดมินไม่มี token → 401
- ปิดถาวรแล้ว → เปิดขายไม่ได้ (409), สั่งซื้อไม่ได้ (`sales_closed`)
- สลิปซ้ำ / สลิปไม่ถูกต้อง → มีแถวใน `rejected_attempts` พร้อมรูป
- ทดสอบตัวอ่าน QR ด้วยรูปสลิปจริง (ยังรอตัวอย่างจากผู้ใช้; ระหว่างนี้ใช้ QR ที่สร้างตามรูปแบบ TLV ใน test)
