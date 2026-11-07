# คู่มือสำหรับทีม Frontend — ระบบขายบัตร Pre-sale บ้านผีสิง

เอกสารนี้บอกว่า frontend (Next.js) ต้องทำหน้าอะไรบ้าง และเรียก backend (Rust) ยังไง
รายละเอียดการออกแบบทั้งหมดอยู่ที่ `docs/superpowers/specs/2026-11-07-ticket-issuing-design.md`

---

## 1. ภาพรวม

- ขายบัตร **ล่วงหน้า** ผ่านเว็บ งานเปิด **วันเดียว** บัตรมีเลข **001–600**
- **เลขบัตรบนเว็บ = เลขบัตรกระดาษจริง = ลำดับคิว** ผู้ซื้อเอาหน้าจอที่โชว์เลขไปแลกบัตรจริงกับทีมงาน
- 1 รหัสนักศึกษา ซื้อได้ 1 ใบ
- ผู้ซื้อโอนเงินเอง แล้วแนบรูปสลิป ระบบอ่าน QR บนสลิป ถ้าเป็นสลิปจริงจะ **ออกเลขบัตรทันที** ไม่ต้องรอแอดมิน
- ระบบ **ไม่ได้** ตรวจยอดเงิน และไม่ได้ตรวจว่าโอนเข้าบัญชีถูก แอดมินจะเปิดดูสลิปแล้วยกเลิกใบที่ผิดปกติเอง

## 2. การเชื่อมต่อ

| | ค่า |
|---|---|
| Base URL (dev) | `http://localhost:8080` |
| รูปแบบข้อมูล | JSON ยกเว้นตอนส่งคำสั่งซื้อ (multipart) และตอนดาวน์โหลดรูปสลิป |
| CORS | backend อนุญาตเฉพาะ origin ที่ตั้งไว้ใน `CORS_ORIGIN` (ค่าเริ่มต้นคือ `http://localhost:3000`) ถ้า frontend รันที่อื่นต้องบอกคนดูแล backend |

แนะนำให้เก็บ base URL ไว้ใน env ของ Next.js:
```
NEXT_PUBLIC_API_URL=http://localhost:8080
```

> backend ไม่มีหน้าเว็บที่ `/` (เปิดแล้วได้ 404 ซึ่งเป็นเรื่องปกติ) ถ้าจะเช็คว่า backend ทำงานอยู่ ให้เรียก `GET /healthz` → `{"status":"ok"}`

### รูปแบบ error (ใช้เหมือนกันทุก endpoint)
```json
{ "code": "already_has_ticket", "message": "รหัสนักศึกษานี้มีบัตรอยู่แล้ว (เลข 012)", "...": "ข้อมูลเพิ่มเติมบางกรณี" }
```
- `message` เป็นภาษาไทย **แสดงให้ผู้ใช้ดูได้เลย**
- ถ้าต้องแยกการทำงานตามชนิด error ให้เช็คจาก `code` อย่าเช็คจาก `message`

---

## 3. หน้าที่ต้องทำ (ฝั่งผู้ซื้อ)

### 3.1 หน้าซื้อบัตร

**เมื่อเปิดหน้า:** เรียก `GET /api/sales`
```json
{ "open": true, "closed_forever": false, "total": 600, "sold": 42, "remaining": 558 }
```
| สถานะ | สิ่งที่ต้องแสดง |
|---|---|
| `open: true` | ฟอร์มซื้อบัตร และโชว์จำนวนที่เหลือ (`remaining`) |
| `open: false` และ `closed_forever: false` | "ยังไม่เปิดขาย / ปิดขายชั่วคราว" |
| `closed_forever: true` | "ปิดขายผ่านเว็บแล้ว ซื้อได้ที่หน้างาน" |
| `remaining: 0` | "บัตรหมดแล้ว" |

**ข้อมูลการโอนเงิน:** backend ไม่ได้ส่งข้อมูลนี้มา frontend ต้องแสดงเอง (เลขบัญชี / QR พร้อมเพย์ / ราคาบัตร) ให้ขอค่าจากผู้จัดงาน

**ฟอร์ม**
| ช่อง | กฎ (backend ตรวจซ้ำอีกรอบ) |
|---|---|
| ชื่อ (`name`) | ห้ามว่าง ยาวไม่เกิน 100 ตัวอักษร |
| รหัสนักศึกษา (`student_id`) | ตัวเลข 10 หลัก (ช่องว่างหน้า-หลังถูกตัดให้อัตโนมัติ) |
| เบอร์โทร (`phone`) | ตัวเลข 10 หลัก ขึ้นต้นด้วย 0 |
| รูปสลิป (`slip`) | JPG / PNG / WebP ขนาดไม่เกิน 5 MB |

⚠️ **ห้ามบีบอัด ย่อ หรือครอปรูปสลิปฝั่ง client** เพราะ QR บนสลิปเล็กมาก บีบนิดเดียวก็อาจอ่านไม่ออก ให้ส่งไฟล์ต้นฉบับไปเลย
แนะนำให้แสดงรูปตัวอย่างหลังผู้ใช้เลือกไฟล์ และบอกผู้ใช้ว่า "ใช้รูปสลิปเต็มใบ ไม่ครอป ต้องเห็น QR"

**ส่งคำสั่งซื้อ:** `POST /api/orders` แบบ `multipart/form-data`
```ts
const fd = new FormData();
fd.append("name", name);
fd.append("student_id", studentId);
fd.append("phone", phone);
fd.append("slip", file); // File จาก <input type="file" accept="image/jpeg,image/png,image/webp">
const res = await fetch(`${API}/api/orders`, { method: "POST", body: fd });
// ห้ามตั้ง Content-Type เอง ให้เบราว์เซอร์ใส่ boundary ให้
```

**สำเร็จ** `201`:
```json
{ "ticket_no": 12, "ticket_label": "012", "name": "สมชาย ใจดี", "student_id": "6831501178", "picked_up": false }
```
→ พาไปหน้าแสดงบัตร (3.2) และแสดง `ticket_label` ตัวใหญ่ ๆ

**ไม่สำเร็จ**
| HTTP | `code` | ความหมาย / สิ่งที่ควรทำ |
|---|---|---|
| 400 | `invalid_input` | ข้อมูลผิดหรือไฟล์ผิดประเภท/ใหญ่เกิน แสดง `message` |
| 403 | `sales_closed` | ปิดขายอยู่ แสดง `message` แล้วโหลดสถานะใหม่ |
| 422 | `invalid_slip` | อ่านสลิปไม่ได้ แสดง `message` + **`contact`** (ช่องทางติดต่อทีมงาน) และให้ลองแนบรูปใหม่ |
| 409 | `duplicate_slip` | สลิปนี้เคยถูกใช้แล้ว แสดง `message` + **`contact`** |
| 409 | `already_has_ticket` | รหัสนี้มีบัตรแล้ว response มี `ticket_label` → แสดงเลขเดิม + ปุ่มไปหน้าดูบัตร |
| 409 | `sold_out` | บัตรหมด |
| 500 | `internal` | ระบบขัดข้อง ให้ลองใหม่ |

⚠️ **กันการกดส่งซ้ำ:** ปิดปุ่มส่งระหว่างรอผล (อ่านสลิปใช้เวลาประมาณ 1 วินาที) ถึงจะกดซ้ำ backend ก็ไม่ออกบัตรซ้ำ แต่ผู้ใช้จะเห็น error `already_has_ticket` ซึ่งทำให้งง

### 3.2 หน้าค้นหา / แสดงบัตร

ผู้ใช้กรอกรหัสนักศึกษา → `GET /api/tickets/{student_id}`
```json
{ "ticket_no": 12, "ticket_label": "012", "name": "สมชาย ใจดี", "student_id": "6831501178", "picked_up": false }
```
- `404 not_found` → "ไม่พบบัตรของรหัสนี้"
- `400 invalid_input` → รหัสไม่ใช่ตัวเลข 10 หลัก
- หน้านี้ผู้ใช้จะเปิดโชว์ทีมงานตอนแลกบัตรจริง **เลขบัตรต้องใหญ่ อ่านง่าย** และแสดงสถานะ `picked_up` (รับบัตรแล้ว / ยังไม่รับ)
- แนะนำให้ใส่รหัสใน URL ได้ (เช่น `/ticket/6831501178`) ผู้ใช้จะได้บุ๊กมาร์กไว้

> หมายเหตุ: ใครรู้รหัสนักศึกษาก็เปิดดูชื่อกับเลขบัตรได้ ระบบตั้งใจให้เป็นแบบนี้ API นี้ไม่ส่งเบอร์โทรออกมา

---

## 4. หน้าแอดมิน

### 4.1 ล็อกอิน
`POST /api/admin/login` body `{ "password": "..." }` → `{ "token": "eyJ..." }`
- token ใช้ได้ **12 ชั่วโมง** เก็บไว้ใน `sessionStorage` หรือ memory
- ทุก request ของแอดมินต้องใส่ header `Authorization: Bearer <token>`
- ถ้าได้ `401 unauthorized` จาก request ใดก็ตาม → ลบ token แล้วพากลับไปหน้าล็อกอิน
- ถ้าได้ `429 too_many_attempts` → ใส่รหัสผิดครบ 5 ครั้งใน 1 นาที response มี `retry_after_secs` ให้แสดงเวลานับถอยหลัง (ระหว่างนี้ใส่รหัสถูกก็เข้าไม่ได้)

### 4.2 แดชบอร์ด
`GET /api/admin/stats`
```json
{
  "total": 600, "sold": 42, "remaining": 558, "cancelled": 3, "picked_up": 10,
  "open": true, "closed_forever": false,
  "free_numbers": [7, 43, 44, 45, "..."]
}
```
- แสดงตัวเลขสรุป
- **`free_numbers` สำคัญมาก** ทีมขายหน้างานใช้ดูว่าเลขบัตรกระดาษไหนยังขายได้ ควรแสดงให้อ่านง่าย เช่นยุบเป็นช่วง `007, 043–600`

### 4.3 เปิด/ปิดการขาย
- `PUT /api/admin/sales` body `{ "open": true | false }` → `{ "open": ..., "closed_forever": ... }`
- `POST /api/admin/sales/close-forever` → ปิด pre-sale **ถาวร ย้อนไม่ได้**
  - ⚠️ ต้องมีหน้าต่างยืนยันก่อนกด เช่นให้พิมพ์คำว่า "ปิดถาวร"
  - ปิดถาวรแล้ว `PUT /api/admin/sales {open:true}` จะได้ `409 presale_closed_forever` ให้ซ่อนปุ่มเปิดขายไปเลย

### 4.4 รายชื่อผู้ซื้อ
`GET /api/admin/orders?q=<คำค้น>&status=<active|cancelled>` (ไม่ใส่พารามิเตอร์ = ทั้งหมด)
- `q` ค้นได้จากชื่อ รหัส เบอร์ (บางส่วนก็ได้) หรือเลขบัตร
- ผลลัพธ์เรียงตามเลขบัตร
```json
[{
  "id": "5b0c…uuid", "ticket_no": 12, "ticket_label": "012", "status": "active",
  "name": "สมชาย ใจดี", "student_id": "6831501178", "phone": "0812345678",
  "slip_path": "orders/…png", "slip_ref": "015123…",
  "created_at": "2026-11-08T03:12:45.123Z",
  "cancelled_at": null, "cancel_reason": null, "picked_up_at": null
}]
```
- เวลาเป็น **UTC (ISO 8601)** ตอนแสดงให้แปลงเป็นเวลาไทย (`Asia/Bangkok`)
- ไม่ต้องแสดง `slip_path` กับ `slip_ref` ในหน้าจอ

**การกระทำต่อแถว**
| ปุ่ม | API | หมายเหตุ |
|---|---|---|
| ดูสลิป | `GET /api/admin/orders/{id}/slip` | ต้องใส่ header token ใช้ `<img src>` ตรง ๆ **ไม่ได้** ดูวิธีด้านล่าง |
| ยกเลิกบัตร | `POST /api/admin/orders/{id}/cancel` body `{ "reason": "สลิปปลอม" }` | ต้องใส่เหตุผล (ว่าง → 400) ควรมีหน้าต่างยืนยัน เลขบัตรใบนั้นจะกลับมาว่าง ถ้ายกเลิกซ้ำจะได้ `409 conflict` |
| รับบัตรแล้ว / ยกเลิกการติ๊ก | `POST /api/admin/orders/{id}/pickup` body `{ "picked_up": true \| false }` | ใช้ตอนแจกบัตรจริง ถ้าบัตรถูกยกเลิกแล้วจะได้ `409` |

ทั้งสองปุ่มจะได้ order ที่อัปเดตแล้วกลับมา (รูปแบบเดียวกับแถวในรายชื่อ)

**แสดงรูปสลิปที่ต้องใช้ token:**
```ts
const res = await fetch(`${API}/api/admin/orders/${id}/slip`, {
  headers: { Authorization: `Bearer ${token}` },
});
const url = URL.createObjectURL(await res.blob());
// <img src={url} />  และเรียก URL.revokeObjectURL(url) ตอนปิด
```

**โหมดแจกบัตรหน้างาน (แนะนำ):** มีช่องค้นหาด้วยรหัสหรือเลขบัตร → แสดงชื่อ เลข และรูปสลิป → ปุ่ม "รับบัตรแล้ว" ปุ่มใหญ่ ๆ ใช้บนมือถือได้

### 4.5 ความพยายามที่ถูกปฏิเสธ
`GET /api/admin/rejected` (ใหม่สุดก่อน)
```json
[{ "id": "…", "reason": "invalid_slip", "name": "…", "student_id": "…", "phone": "…",
   "slip_path": "rejected/…png", "slip_ref": null, "created_at": "…" }]
```
- `reason` มีได้ 3 ค่า: `invalid_slip` (อ่านสลิปไม่ได้), `duplicate_slip` (ใช้สลิปซ้ำ อาจเป็นการโกง), `already_has_ticket` (รหัสนี้ซื้อซ้ำ)
- ดูรูปได้ที่ `GET /api/admin/rejected/{id}/slip` ใช้วิธีเดียวกับข้อ 4.4
- ใช้ตอนมีคนทักมาว่า "โอนแล้วแต่ระบบไม่รับ" แอดมินจะเปิดดูรูปที่เขาส่งมาได้

---

## 5. สรุป endpoint ทั้งหมด

| Method | Path | Auth | ใช้ในหน้า |
|---|---|---|---|
| GET | `/healthz` | – | (เช็คว่า backend ทำงานอยู่) |
| GET | `/api/sales` | – | ซื้อบัตร |
| POST | `/api/orders` | – | ซื้อบัตร |
| GET | `/api/tickets/{student_id}` | – | ค้นบัตร |
| POST | `/api/admin/login` | – | ล็อกอินแอดมิน |
| GET | `/api/admin/stats` | ✅ | แดชบอร์ด |
| GET | `/api/admin/orders` | ✅ | รายชื่อ |
| GET | `/api/admin/orders/{id}/slip` | ✅ | ดูสลิป |
| POST | `/api/admin/orders/{id}/cancel` | ✅ | ยกเลิกบัตร |
| POST | `/api/admin/orders/{id}/pickup` | ✅ | แจกบัตร |
| PUT | `/api/admin/sales` | ✅ | เปิด/ปิดขาย |
| POST | `/api/admin/sales/close-forever` | ✅ | ปิดถาวร |
| GET | `/api/admin/rejected` | ✅ | ความพยายามที่ถูกปฏิเสธ |
| GET | `/api/admin/rejected/{id}/slip` | ✅ | ดูรูปที่ถูกปฏิเสธ |

## 6. ทดสอบระหว่างพัฒนา

```bash
# ในโฟลเดอร์ H-Ticket-Backend
docker compose up -d --build
```
- รูปสลิปปลอมที่ระบบรับ: `tests/fixtures/sample-slip.png`, `tests/fixtures/sample-slip-2.png` (แต่ละรูปใช้ได้ครั้งเดียว)
- สร้างรูปใหม่: `cargo run --example make_sample_slip -- REF-ใดก็ได้ /tmp/slip.png`
- การขายเริ่มต้นเป็น **ปิด** ต้องล็อกอินแอดมินแล้วเปิดขายก่อน จึงจะทดสอบการซื้อได้
- ล้างข้อมูลทั้งหมด: `docker compose down -v`
