use crate::db::{orders, orders::Order, rejected, settings};
use crate::error::AppError;
use crate::storage::SlipStore;
use crate::validate::{NewOrderInput, detect_image, normalize_student_id, validate_group};
use crate::{AppState, slip};
use axum::{
    Json,
    extract::{Multipart, Path, State, multipart::MultipartRejection},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// ข้อมูลการจองที่เปิดเผยได้ (ไม่มีเบอร์โทร) · ไม่มีเลขบัตร รับบัตรจริงที่หน้างาน
pub fn ticket_view(o: &Order) -> Value {
    json!({
        "name": o.name,
        "student_id": o.student_id,
        "picked_up": o.picked_up_at.is_some(),
        "group_size": o.group_size,
    })
}

/// รายชื่อเพื่อนร่วมกลุ่ม (ชื่อ + สถานะรับบัตร ไม่มีรหัสนักศึกษา/เบอร์)
fn group_view(group: &[Order]) -> Value {
    group
        .iter()
        .map(|o| json!({ "name": o.name, "picked_up": o.picked_up_at.is_some() }))
        .collect()
}

pub async fn sales(State(st): State<AppState>) -> Result<Json<Value>, AppError> {
    let s = settings::get_sales_state(&st.pool).await?;
    let stats = orders::stats(&st.pool, st.config.total_tickets).await?;
    Ok(Json(json!({
        "open": s.open,
        "closed_forever": s.closed_forever,
        "total": stats.total,
        "sold": stats.sold,
        "remaining": stats.remaining,
    })))
}

pub async fn ticket(
    State(st): State<AppState>,
    Path(student_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let sid = normalize_student_id(&student_id)
        .ok_or_else(|| AppError::InvalidInput("รหัสนักศึกษาต้องเป็นตัวเลข 10 หลัก".into()))?;
    let order = orders::find_active_by_student(&st.pool, &sid)
        .await?
        .ok_or(AppError::NotFound)?;
    let mut view = ticket_view(&order);
    if order.group_size > 1 {
        view["group"] = group_view(&orders::list_group(&st.pool, order.group_id).await?);
    }
    Ok(Json(view))
}

fn multipart_err(e: axum::extract::multipart::MultipartError) -> AppError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::InvalidInput("ไฟล์ใหญ่เกิน 5 MB".into())
    } else {
        AppError::InvalidInput("ข้อมูลที่ส่งมาไม่ถูกต้อง".into())
    }
}

/// ฟิลด์ name/student_id ส่งซ้ำได้ตามจำนวนคน (เรียงคู่กัน) ส่วน phone เป็นของผู้ซื้อคนเดียว
struct Form {
    names: Vec<String>,
    student_ids: Vec<String>,
    phone: String,
    slip: Vec<u8>,
}

async fn read_form(mut mp: Multipart) -> Result<Form, AppError> {
    let (mut names, mut sids, mut phone, mut slip) = (Vec::new(), Vec::new(), None, None);
    while let Some(field) = mp.next_field().await.map_err(multipart_err)? {
        match field.name().unwrap_or_default() {
            "name" => names.push(field.text().await.map_err(multipart_err)?),
            "student_id" => sids.push(field.text().await.map_err(multipart_err)?),
            "phone" => phone = Some(field.text().await.map_err(multipart_err)?),
            "slip" => slip = Some(field.bytes().await.map_err(multipart_err)?.to_vec()),
            _ => {}
        }
        if names.len() > 5 || sids.len() > 5 {
            return Err(AppError::InvalidInput("จำนวนคนต้องเป็น 1, 2 หรือ 5 คน".into()));
        }
    }
    let missing = || AppError::InvalidInput("กรุณากรอกข้อมูลและแนบสลิปให้ครบ".into());
    if names.is_empty() || names.len() != sids.len() {
        return Err(missing());
    }
    Ok(Form {
        names,
        student_ids: sids,
        phone: phone.ok_or_else(missing)?,
        slip: slip.ok_or_else(missing)?,
    })
}

/// กลุ่มหลายคนบันทึกเป็นแถวเดียว ชื่อและรหัสคั่นด้วย ", "
async fn log_rejected(
    st: &AppState,
    reason: &str,
    inputs: &[NewOrderInput],
    slip_path: Option<&str>,
    slip_ref: Option<&str>,
) {
    let join = |f: fn(&NewOrderInput) -> &str| inputs.iter().map(f).collect::<Vec<_>>().join(", ");
    if let Err(e) = rejected::insert(
        &st.pool,
        reason,
        Some(&join(|i| &i.name)),
        Some(&join(|i| &i.student_id)),
        Some(&inputs[0].phone),
        slip_path,
        slip_ref,
    )
    .await
    {
        tracing::error!(error = ?e, "failed to log rejected attempt");
    }
}

pub async fn create_order(
    State(st): State<AppState>,
    mp: Result<Multipart, MultipartRejection>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let mp = mp.map_err(|_| AppError::InvalidInput("ต้องส่งข้อมูลแบบ multipart/form-data".into()))?;
    let form = read_form(mp).await?;
    let people: Vec<(String, String)> = form.names.into_iter().zip(form.student_ids).collect();
    let inputs = validate_group(&people, &form.phone)?;
    let kind = detect_image(&form.slip)?;

    if !settings::get_sales_state(&st.pool).await?.open {
        return Err(AppError::SalesClosed);
    }

    let store = SlipStore::new(&st.config.slip_dir);
    let order_id = Uuid::new_v4();
    let file_name = format!("{order_id}.{}", kind.ext());

    // ถอดรหัสรูปพร้อมกันได้ไม่เกิน 2 รูป เพื่อจำกัดหน่วยความจำ
    static DECODE_PERMITS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let permit = DECODE_PERMITS
        .acquire()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let bytes = form.slip;
    let (bytes, read) = tokio::task::spawn_blocking(move || {
        let r = slip::read_slip_ref(&bytes);
        (bytes, r)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    drop(permit);

    let slip_ref = match read {
        Ok(r) => r,
        Err(_) => {
            let path = store.save_rejected(&bytes, &file_name).await.ok();
            log_rejected(&st, "invalid_slip", &inputs, path.as_deref(), None).await;
            return Err(AppError::InvalidSlip);
        }
    };

    let temp = store.save_temp(&bytes).await?;
    let slip_path = format!("orders/{file_name}");
    match orders::issue_tickets(
        &st.pool,
        st.config.total_tickets,
        order_id,
        &inputs,
        &slip_ref,
        &slip_path,
    )
    .await
    {
        Ok(orders) => {
            store.finalize(&temp, &file_name).await?;
            // ตัวแรกคือบัตรของผู้ซื้อ (คงรูปแบบเดิม) + tickets ครบทุกคน
            let mut body = ticket_view(&orders[0]);
            body["tickets"] = orders.iter().map(ticket_view).collect();
            Ok((StatusCode::CREATED, Json(body)))
        }
        Err(e @ (AppError::DuplicateSlip | AppError::AlreadyHasTicket { .. })) => {
            let path = store.reject_temp(&temp, &file_name).await.ok();
            log_rejected(&st, e.code(), &inputs, path.as_deref(), Some(&slip_ref)).await;
            Err(e)
        }
        Err(e) => {
            store.remove(&temp).await;
            Err(e)
        }
    }
}
