use crate::error::AppError;

pub const MAX_SLIP_BYTES: usize = 5 * 1024 * 1024;
/// จำนวนคนต่อออเดอร์ที่ขายได้ (โปร 1 คน / 2 คน / 5 คน)
pub const GROUP_SIZES: [usize; 3] = [1, 2, 5];

#[derive(Debug, Clone)]
pub struct NewOrderInput {
    pub name: String,
    pub student_id: String,
    pub phone: String,
}

fn bad(msg: &str) -> AppError {
    AppError::InvalidInput(msg.to_string())
}

fn is_digits(s: &str, len: usize) -> bool {
    s.len() == len && s.bytes().all(|b| b.is_ascii_digit())
}

/// รหัสนักศึกษา: ตัดช่องว่าง แล้วต้องเป็นตัวเลข 10 หลัก
pub fn normalize_student_id(s: &str) -> Option<String> {
    let s = s.trim();
    is_digits(s, 10).then(|| s.to_string())
}

pub fn validate_order(
    name: &str,
    student_id: &str,
    phone: &str,
) -> Result<NewOrderInput, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(bad("กรุณากรอกชื่อ"));
    }
    if name.chars().any(char::is_control) {
        return Err(bad("ชื่อมีอักขระที่ไม่อนุญาต"));
    }
    if name.chars().count() > 100 {
        return Err(bad("ชื่อยาวเกิน 100 ตัวอักษร"));
    }
    let student_id =
        normalize_student_id(student_id).ok_or_else(|| bad("รหัสนักศึกษาต้องเป็นตัวเลข 10 หลัก"))?;
    let phone = phone.trim();
    if !is_digits(phone, 10) || !phone.starts_with('0') {
        return Err(bad("เบอร์โทรต้องเป็นตัวเลข 10 หลัก ขึ้นต้นด้วย 0"));
    }
    Ok(NewOrderInput {
        name: name.to_string(),
        student_id,
        phone: phone.to_string(),
    })
}

/// ตรวจข้อมูลทุกคนในกลุ่ม: จำนวนคนต้องตรงกับโปร และรหัสนักศึกษาห้ามซ้ำกันเอง
/// เบอร์โทรเป็นของผู้ซื้อ ใช้ร่วมกันทั้งกลุ่ม
pub fn validate_group(
    people: &[(String, String)],
    phone: &str,
) -> Result<Vec<NewOrderInput>, AppError> {
    if !GROUP_SIZES.contains(&people.len()) {
        return Err(bad("จำนวนคนต้องเป็น 1, 2 หรือ 5 คน"));
    }
    let mut out: Vec<NewOrderInput> = Vec::with_capacity(people.len());
    for (name, sid) in people {
        let input = validate_order(name, sid, phone)?;
        if out.iter().any(|o| o.student_id == input.student_id) {
            return Err(bad(&format!(
                "รหัสนักศึกษา {} ซ้ำกันในกลุ่ม",
                input.student_id
            )));
        }
        out.push(input);
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg,
    Png,
    Webp,
}

impl ImageKind {
    pub fn ext(self) -> &'static str {
        match self {
            ImageKind::Jpeg => "jpg",
            ImageKind::Png => "png",
            ImageKind::Webp => "webp",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            ImageKind::Jpeg => "image/jpeg",
            ImageKind::Png => "image/png",
            ImageKind::Webp => "image/webp",
        }
    }

    pub fn from_ext(ext: &str) -> Option<ImageKind> {
        match ext {
            "jpg" => Some(ImageKind::Jpeg),
            "png" => Some(ImageKind::Png),
            "webp" => Some(ImageKind::Webp),
            _ => None,
        }
    }
}

/// ตรวจชนิดรูปจาก magic bytes และขนาดไฟล์
pub fn detect_image(bytes: &[u8]) -> Result<ImageKind, AppError> {
    if bytes.len() > MAX_SLIP_BYTES {
        return Err(bad("ไฟล์ใหญ่เกิน 5 MB"));
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Ok(ImageKind::Jpeg)
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Ok(ImageKind::Png)
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Ok(ImageKind::Webp)
    } else {
        Err(bad("กรุณาแนบรูปสลิปเป็นไฟล์ JPG, PNG หรือ WebP"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_input_is_trimmed() {
        let v = validate_order("  สมชาย ใจดี ", " 6831501178 ", "0812345678").unwrap();
        assert_eq!(v.name, "สมชาย ใจดี");
        assert_eq!(v.student_id, "6831501178");
        assert_eq!(v.phone, "0812345678");
    }

    #[test]
    fn rejects_bad_fields() {
        let ok_id = "6831501178";
        let ok_phone = "0812345678";
        assert!(validate_order("   ", ok_id, ok_phone).is_err());
        assert!(validate_order(&"ก".repeat(101), ok_id, ok_phone).is_err());
        assert!(validate_order(&"ก".repeat(100), ok_id, ok_phone).is_ok());
        assert!(validate_order("A", "683150117", ok_phone).is_err());
        assert!(validate_order("A", "68315011a8", ok_phone).is_err());
        assert!(validate_order("A", ok_id, "081234567").is_err());
        assert!(validate_order("A", ok_id, "1812345678").is_err());
    }

    #[test]
    fn errors_are_invalid_input() {
        let e = validate_order("", "1", "1").unwrap_err();
        assert_eq!(e.code(), "invalid_input");
    }

    #[test]
    fn rejects_control_chars_in_name() {
        assert!(validate_order("A\0B", "6831501178", "0812345678").is_err());
        assert!(validate_order("A\nB", "6831501178", "0812345678").is_err());
    }

    #[test]
    fn group_sizes_and_duplicates() {
        let p = |n: &str, id: &str| (n.to_string(), id.to_string());
        let phone = "0812345678";
        assert!(validate_group(&[p("A", "6800000001")], phone).is_ok());
        let two = [p("A", "6800000001"), p("B", "6800000002")];
        assert_eq!(validate_group(&two, phone).unwrap().len(), 2);
        assert!(validate_group(&[], phone).is_err());
        let three: Vec<_> = (1..=3).map(|i| p("A", &format!("680000000{i}"))).collect();
        assert!(validate_group(&three, phone).is_err());
        let dup = [p("A", "6800000001"), p("B", " 6800000001")];
        assert!(validate_group(&dup, phone).is_err());
    }

    #[test]
    fn normalize_student_id_trims() {
        assert_eq!(
            normalize_student_id(" 6831501178\n"),
            Some("6831501178".into())
        );
        assert_eq!(normalize_student_id("abc"), None);
    }

    #[test]
    fn detects_image_types() {
        assert_eq!(
            detect_image(&[0xFF, 0xD8, 0xFF, 0xE0, 0]).unwrap(),
            ImageKind::Jpeg
        );
        assert_eq!(
            detect_image(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0]).unwrap(),
            ImageKind::Png
        );
        assert_eq!(
            detect_image(b"RIFF\0\0\0\0WEBPVP8 ").unwrap(),
            ImageKind::Webp
        );
        assert_eq!(ImageKind::Jpeg.ext(), "jpg");
        assert_eq!(ImageKind::Png.mime(), "image/png");
    }

    #[test]
    fn rejects_non_images_and_big_files() {
        assert!(detect_image(b"%PDF-1.7").is_err());
        assert!(detect_image(b"").is_err());
        let mut big = vec![0u8; 5 * 1024 * 1024 + 1];
        big[..3].copy_from_slice(&[0xFF, 0xD8, 0xFF]);
        assert!(detect_image(&big).is_err());
        big.truncate(5 * 1024 * 1024);
        assert!(detect_image(&big).is_ok());
    }
}
