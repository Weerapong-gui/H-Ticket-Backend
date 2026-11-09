-- ซื้อเป็นกลุ่ม (โปร 1/2/5 คน): หลายบัตรใช้สลิปเดียวกัน ผูกกันด้วย group_id
ALTER TABLE orders ADD COLUMN group_id uuid, ADD COLUMN group_size smallint NOT NULL DEFAULT 1;
UPDATE orders SET group_id = id;
ALTER TABLE orders ALTER COLUMN group_id SET NOT NULL;
-- สลิปเดียวใช้ได้หลายแถวในกลุ่มเดียวกัน (กันสลิปซ้ำข้ามกลุ่มใน issue_tickets ภายใต้ ISSUE_LOCK)
ALTER TABLE orders DROP CONSTRAINT orders_slip_ref_key;
CREATE INDEX orders_slip_ref ON orders (slip_ref);
CREATE INDEX orders_group ON orders (group_id);
