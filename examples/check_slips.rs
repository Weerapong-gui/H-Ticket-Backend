//! ตรวจรูปสลิปจริงทั้งโฟลเดอร์ ว่าตัวอ่าน QR รับได้ไหม
//! cargo run --release --example check_slips -- data/real-slips
use h_ticket_backend::slip::{SlipError, parse_slip_payload, read_slip_ref};
use std::time::Instant;

fn mask(s: &str) -> String {
    let n = s.chars().count();
    s.chars()
        .enumerate()
        .map(|(i, c)| if i < 4 || i + 4 >= n { c } else { '*' })
        .collect()
}

/// โครงสร้าง TLV แบบย่อ: tag(ความยาว) และค่าของ tag สั้น ๆ ที่ไม่ใช่ข้อมูลส่วนตัว
fn shape(p: &str) -> String {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= p.len() {
        let (Some(tag), Some(len)) = (
            p.get(i..i + 2),
            p.get(i + 2..i + 4).and_then(|l| l.parse::<usize>().ok()),
        ) else {
            out.push("<parse-error>".into());
            break;
        };
        let Some(v) = p.get(i + 4..i + 4 + len) else {
            out.push(format!("{tag}(<truncated {len}>)"));
            break;
        };
        out.push(if tag == "00" || len > 8 {
            format!("{tag}({len})")
        } else {
            format!("{tag}={v}")
        });
        i += 4 + len;
    }
    out.join(" ")
}

fn first_payload(bytes: &[u8]) -> Option<String> {
    let img = image::load_from_memory(bytes).ok()?;
    let mut prep = rqrr::PreparedImage::prepare(img.to_luma8());
    prep.detect_grids()
        .into_iter()
        .find_map(|g| g.decode().ok().map(|(_, c)| c))
}

/// รหัสธนาคารผู้ส่ง = sub-tag 01 ใน tag 00
fn bank_code(p: &str) -> String {
    let inner = p
        .get(
            4..4 + p
                .get(2..4)
                .and_then(|l| l.parse::<usize>().ok())
                .unwrap_or(0),
        )
        .unwrap_or("");
    let mut i = 0;
    while i + 4 <= inner.len() {
        let tag = &inner[i..i + 2];
        let Some(len) = inner
            .get(i + 2..i + 4)
            .and_then(|l| l.parse::<usize>().ok())
        else {
            break;
        };
        let v = inner.get(i + 4..i + 4 + len).unwrap_or("");
        if tag == "01" {
            return v.to_string();
        }
        i += 4 + len;
    }
    "?".into()
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: check_slips <dir>");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect();
    files.sort();
    let (mut ok, mut total) = (0, 0);
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        if path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'))
        {
            continue;
        }
        total += 1;
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let t = Instant::now();
        let res = read_slip_ref(&bytes);
        let ms = t.elapsed().as_millis();
        let dims = image::image_dimensions(&path)
            .map(|(w, h)| format!("{w}x{h}"))
            .unwrap_or("?".into());
        match res {
            Ok(r) => {
                ok += 1;
                let payload = first_payload(&bytes).unwrap_or_default();
                println!(
                    "✅ {name:<14} {dims:<10} {ms:>5}ms bank={} ref={} shape: {}",
                    bank_code(&payload),
                    mask(&r),
                    shape(&payload)
                );
            }
            Err(e) => {
                println!("❌ {name:<14} {dims:<10} {ms:>5}ms {e:?}");
                // แสดงเนื้อหา QR ทุกอันที่เจอ เพื่อดูว่าทำไมไม่ผ่าน
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let mut prep = rqrr::PreparedImage::prepare(img.to_luma8());
                    for g in prep.detect_grids() {
                        match g.decode() {
                            Ok((_, c)) => println!(
                                "     QR len={} parse={:?} shape: {}",
                                c.len(),
                                parse_slip_payload(&c).map(|r| mask(&r)),
                                shape(&c)
                            ),
                            Err(e) => println!("     QR grid found but decode failed: {e:?}"),
                        }
                    }
                }
                let _ = SlipError::NoQr;
            }
        }
    }
    println!("\nผ่าน {ok}/{total}");
}
