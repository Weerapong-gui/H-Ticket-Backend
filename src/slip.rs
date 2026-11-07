//! ตรวจสลิปโอนเงินธนาคารไทยจาก mini-QR (รูปแบบ EMVCo TLV)
//!
//! QR บนสลิปมีหน้าตาประมาณ `0041 0006000001 0103014 02xx<ref> 5102TH 9104<crc>`
//! เราถือว่าเป็นสลิปจริงเมื่อ TLV ถูกต้อง, มี transaction ref (tag 00 → 02),
//! มีรหัสประเทศ TH (tag 51) และ CRC16 (tag 91) ถูกต้อง

use image::{DynamicImage, GenericImageView};

/// เพดานความกว้าง/สูงของรูป กันรูปที่บีบอัดเล็กแต่คลายแล้วกินแรมมหาศาล
pub const MAX_IMAGE_DIM: u32 = 10_000;
/// เพดานหน่วยความจำต่อการถอดรหัสรูปหนึ่งรูป
const MAX_DECODE_ALLOC: u64 = 300 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum SlipError {
    /// ไม่ใช่รูป หรือหา QR ในรูปไม่เจอ
    NoQr,
    /// เจอ QR แต่ไม่ใช่รูปแบบสลิป
    NotSlip,
}

/// CRC-16/CCITT-FALSE (poly 0x1021, init 0xFFFF)
fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &b in data {
        crc ^= (b as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

/// แยก TLV ทั้งสาย คืน (tag, value, offset ของ value) หรือ None ถ้าโครงสร้างไม่ครบ
fn parse_tlv(s: &str) -> Option<Vec<(&str, &str, usize)>> {
    if !s.is_ascii() {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let tag = s.get(i..i + 2)?;
        let len_str = s.get(i + 2..i + 4)?;
        if !tag.bytes().all(|b| b.is_ascii_digit()) || !len_str.bytes().all(|b| b.is_ascii_digit())
        {
            return None;
        }
        let len: usize = len_str.parse().ok()?;
        let value = s.get(i + 4..i + 4 + len)?;
        out.push((tag, value, i + 4));
        i += 4 + len;
    }
    Some(out)
}

/// ตรวจ payload ของ QR ว่าเป็นสลิป คืน transaction ref
pub fn parse_slip_payload(payload: &str) -> Result<String, SlipError> {
    let payload = payload.trim();
    let fields = parse_tlv(payload).ok_or(SlipError::NotSlip)?;

    let (last_tag, crc_value, crc_offset) = *fields.last().ok_or(SlipError::NotSlip)?;
    if last_tag != "91" || crc_value.len() != 4 {
        return Err(SlipError::NotSlip);
    }
    let expected = u16::from_str_radix(crc_value, 16).map_err(|_| SlipError::NotSlip)?;
    if crc16_ccitt(&payload.as_bytes()[..crc_offset]) != expected {
        return Err(SlipError::NotSlip);
    }

    if !fields.iter().any(|(t, v, _)| *t == "51" && *v == "TH") {
        return Err(SlipError::NotSlip);
    }

    let (_, inner, _) = fields
        .iter()
        .find(|(t, _, _)| *t == "00")
        .ok_or(SlipError::NotSlip)?;
    let sub = parse_tlv(inner).ok_or(SlipError::NotSlip)?;
    sub.iter()
        .find(|(t, v, _)| *t == "02" && !v.is_empty())
        .map(|(_, v, _)| v.to_string())
        .ok_or(SlipError::NotSlip)
}

/// สร้าง payload สลิปที่ถูกต้อง (ใช้ในเทสต์และสร้างรูปตัวอย่าง)
pub fn encode_slip_payload(trans_ref: &str) -> String {
    let sub = format!("0006000001010301402{:02}{}", trans_ref.len(), trans_ref);
    let body = format!("00{:02}{}5102TH9104", sub.len(), sub);
    let crc = crc16_ccitt(body.as_bytes());
    format!("{body}{crc:04X}")
}

/// หา QR ทุกอันในรูป คืน Ok(ref) ของอันแรกที่เป็นสลิป
fn scan(img: &DynamicImage) -> Result<String, SlipError> {
    let luma = img.to_luma8();
    let mut prepared = rqrr::PreparedImage::prepare(luma);
    let grids = prepared.detect_grids();
    if grids.is_empty() {
        return Err(SlipError::NoQr);
    }
    let mut found_any = false;
    for g in grids {
        if let Ok((_, content)) = g.decode() {
            found_any = true;
            if let Ok(r) = parse_slip_payload(&content) {
                return Ok(r);
            }
        }
    }
    Err(if found_any {
        SlipError::NotSlip
    } else {
        SlipError::NoQr
    })
}

/// อ่าน transaction ref จากรูปสลิป
pub fn read_slip_ref(image_bytes: &[u8]) -> Result<String, SlipError> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_DIM);
    limits.max_image_height = Some(MAX_IMAGE_DIM);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = image::ImageReader::new(std::io::Cursor::new(image_bytes))
        .with_guessed_format()
        .map_err(|_| SlipError::NoQr)?;
    reader.limits(limits);
    let img = reader.decode().map_err(|_| SlipError::NoQr)?;
    match scan(&img) {
        Ok(r) => Ok(r),
        Err(first_err) => {
            // รูปจากมือถือใหญ่มาก ลองย่อแล้วอ่านใหม่
            let (w, h) = img.dimensions();
            if w.max(h) > 1600 {
                let small = img.resize(1600, 1600, image::imageops::FilterType::Triangle);
                match scan(&small) {
                    Ok(r) => Ok(r),
                    Err(SlipError::NotSlip) => Err(SlipError::NotSlip),
                    Err(SlipError::NoQr) => Err(first_err),
                }
            } else {
                Err(first_err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, ImageFormat, Luma};
    use std::io::Cursor;

    fn png_of(img: &GrayImage) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn qr_image(payload: &str) -> GrayImage {
        qrcode::QrCode::new(payload.as_bytes())
            .unwrap()
            .render::<Luma<u8>>()
            .min_dimensions(250, 250)
            .build()
    }

    #[test]
    fn crc16_known_vector() {
        assert_eq!(crc16_ccitt(b"123456789"), 0x29B1);
    }

    #[test]
    fn roundtrip() {
        let p = encode_slip_payload("015123100512ABC");
        assert_eq!(parse_slip_payload(&p), Ok("015123100512ABC".to_string()));
    }

    #[test]
    fn bad_crc_rejected() {
        let mut p = encode_slip_payload("015123100512ABC");
        let last = p.pop().unwrap();
        p.push(if last == '0' { '1' } else { '0' });
        assert_eq!(parse_slip_payload(&p), Err(SlipError::NotSlip));
    }

    #[test]
    fn promptpay_payment_qr_rejected() {
        let p = "00020101021129370016A000000677010111011300668123456785802TH530376463043B5F";
        assert_eq!(parse_slip_payload(p), Err(SlipError::NotSlip));
    }

    #[test]
    fn url_qr_rejected() {
        assert_eq!(
            parse_slip_payload("https://example.com"),
            Err(SlipError::NotSlip)
        );
    }

    #[test]
    fn image_without_qr() {
        let blank = GrayImage::from_pixel(200, 200, Luma([255]));
        assert_eq!(read_slip_ref(&png_of(&blank)), Err(SlipError::NoQr));
        assert_eq!(read_slip_ref(b"not an image at all"), Err(SlipError::NoQr));
    }

    #[test]
    fn image_with_slip_qr() {
        let img = qr_image(&encode_slip_payload("REF1"));
        assert_eq!(read_slip_ref(&png_of(&img)), Ok("REF1".to_string()));
    }

    #[test]
    fn image_with_non_slip_qr_is_not_slip() {
        let img = qr_image("https://example.com");
        assert_eq!(read_slip_ref(&png_of(&img)), Err(SlipError::NotSlip));
    }

    #[test]
    fn oversized_dimensions_rejected_before_decode() {
        // QR ถูกต้อง แต่รูปกว้างเกินเพดาน → ต้องไม่ถอดรหัส (กัน image bomb)
        let qr = qr_image(&encode_slip_payload("REF-WIDE"));
        let mut canvas = GrayImage::from_pixel(MAX_IMAGE_DIM + 1, 300, Luma([255]));
        image::imageops::overlay(&mut canvas, &qr, 10, 10);
        assert_eq!(read_slip_ref(&png_of(&canvas)), Err(SlipError::NoQr));
    }

    #[test]
    fn small_qr_in_large_photo() {
        let qr = qr_image(&encode_slip_payload("REF-BIG"));
        let mut canvas = GrayImage::from_pixel(2000, 3000, Luma([230]));
        image::imageops::overlay(&mut canvas, &qr, 1500, 2500);
        assert_eq!(read_slip_ref(&png_of(&canvas)), Ok("REF-BIG".to_string()));
    }
}
