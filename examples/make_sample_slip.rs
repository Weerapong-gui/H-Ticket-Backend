//! สร้างรูปสลิปปลอม (QR ถูกรูปแบบ) ไว้ทดสอบด้วยมือ
//! cargo run --example make_sample_slip -- <transaction-ref> <output.png>
use image::Luma;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (r, out) = match args.as_slice() {
        [_, r, out] => (r.as_str(), out.as_str()),
        _ => {
            eprintln!("usage: make_sample_slip <transaction-ref> <output.png>");
            std::process::exit(2);
        }
    };
    let payload = h_ticket_backend::slip::encode_slip_payload(r);
    qrcode::QrCode::new(payload.as_bytes())
        .unwrap()
        .render::<Luma<u8>>()
        .min_dimensions(300, 300)
        .build()
        .save(out)
        .unwrap();
    println!("wrote {out} ({payload})");
}
