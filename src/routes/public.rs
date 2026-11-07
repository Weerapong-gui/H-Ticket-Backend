use crate::db::{orders, orders::Order, rejected, settings};
use crate::error::{AppError, ticket_label};
use crate::storage::SlipStore;
use crate::validate::{NewOrderInput, detect_image, normalize_student_id, validate_order};
use crate::{AppState, slip};
use axum::{
    Json,
    extract::{Multipart, Path, State, multipart::MultipartRejection},
    http::StatusCode,
};
use serde_json::{Value, json};
use uuid::Uuid;

/// ข้อมูลบัตรที่เปิดเผยได้ (ไม่มีเบอร์โทร)
pub fn ticket_view(o: &Order) -> Value {
    json!({
        "ticket_no": o.ticket_no,
        "ticket_label": ticket_label(o.ticket_no),
        "name": o.name,
        "student_id": o.student_id,
        "picked_up": o.picked_up_at.is_some(),
    })
}

pub async fn sales(State(st): State<AppState>) -> Result<Json<Value>, AppError> {
    let s = settings::get_sales_state(&st.pool).await?;
    let stats = orders::stats(&st.pool, st.config.total_tickets).await?;
    Ok(Json(json!({
        "open": s.open,
        "closed_forever": s.closed_forever,
        "total": st.config.total_tickets,
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
    Ok(Json(ticket_view(&order)))
}

fn multipart_err(e: axum::extract::multipart::MultipartError) -> AppError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        AppError::InvalidInput("ไฟล์ใหญ่เกิน 5 MB".into())
    } else {
        AppError::InvalidInput("ข้อมูลที่ส่งมาไม่ถูกต้อง".into())
    }
}

struct Form {
    name: String,
    student_id: String,
    phone: String,
    slip: Vec<u8>,
}

async fn read_form(mut mp: Multipart) -> Result<Form, AppError> {
    let (mut name, mut sid, mut phone, mut slip) = (None, None, None, None);
    while let Some(field) = mp.next_field().await.map_err(multipart_err)? {
        match field.name().unwrap_or_default() {
            "name" => name = Some(field.text().await.map_err(multipart_err)?),
            "student_id" => sid = Some(field.text().await.map_err(multipart_err)?),
            "phone" => phone = Some(field.text().await.map_err(multipart_err)?),
            "slip" => slip = Some(field.bytes().await.map_err(multipart_err)?.to_vec()),
            _ => {}
        }
    }
    let missing = || AppError::InvalidInput("กรุณากรอกข้อมูลและแนบสลิปให้ครบ".into());
    Ok(Form {
        name: name.ok_or_else(missing)?,
        student_id: sid.ok_or_else(missing)?,
        phone: phone.ok_or_else(missing)?,
        slip: slip.ok_or_else(missing)?,
    })
}

async fn log_rejected(
    st: &AppState,
    reason: &str,
    input: &NewOrderInput,
    slip_path: Option<&str>,
    slip_ref: Option<&str>,
) {
    if let Err(e) = rejected::insert(
        &st.pool,
        reason,
        Some(&input.name),
        Some(&input.student_id),
        Some(&input.phone),
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
    let input = validate_order(&form.name, &form.student_id, &form.phone)?;
    let kind = detect_image(&form.slip)?;

    if !settings::get_sales_state(&st.pool).await?.open {
        return Err(AppError::SalesClosed);
    }

    let store = SlipStore::new(&st.config.slip_dir);
    let order_id = Uuid::new_v4();
    let file_name = format!("{order_id}.{}", kind.ext());

    let bytes = form.slip;
    let (bytes, read) = tokio::task::spawn_blocking(move || {
        let r = slip::read_slip_ref(&bytes);
        (bytes, r)
    })
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    let slip_ref = match read {
        Ok(r) => r,
        Err(_) => {
            let path = store.save_rejected(&bytes, &file_name).await.ok();
            log_rejected(&st, "invalid_slip", &input, path.as_deref(), None).await;
            return Err(AppError::InvalidSlip);
        }
    };

    let temp = store.save_temp(&bytes).await?;
    let slip_path = format!("orders/{file_name}");
    match orders::issue_ticket(
        &st.pool,
        st.config.total_tickets,
        order_id,
        &input,
        &slip_ref,
        &slip_path,
    )
    .await
    {
        Ok(order) => {
            store.finalize(&temp, &file_name).await?;
            Ok((StatusCode::CREATED, Json(ticket_view(&order))))
        }
        Err(e @ (AppError::DuplicateSlip | AppError::AlreadyHasTicket { .. })) => {
            let path = store.reject_temp(&temp, &file_name).await.ok();
            log_rejected(&st, e.code(), &input, path.as_deref(), Some(&slip_ref)).await;
            Err(e)
        }
        Err(e) => {
            store.remove(&temp).await;
            Err(e)
        }
    }
}
