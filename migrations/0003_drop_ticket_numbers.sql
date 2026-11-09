-- ไม่ออกเลขบัตรจากเว็บแล้ว: staff หยิบบัตรใบถัดไปจากกองที่หน้างาน แล้วติ๊ก "รับบัตรแล้ว"
-- จำนวนบัตรนับจากรายการที่ยัง active แทนเลขบัตร
DROP INDEX IF EXISTS orders_active_ticket;
ALTER TABLE orders DROP COLUMN ticket_no;
