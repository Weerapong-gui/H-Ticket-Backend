use h_ticket_backend::db::{orders, rejected, settings};
use h_ticket_backend::error::AppError;
use h_ticket_backend::validate::NewOrderInput;
use sqlx::PgPool;
use uuid::Uuid;

fn input(n: u32) -> NewOrderInput {
    NewOrderInput {
        name: format!("คนที่ {n}"),
        student_id: format!("68{n:08}"),
        phone: "0812345678".into(),
    }
}

async fn issue(
    pool: &PgPool,
    total: i16,
    n: u32,
    slip_ref: &str,
) -> Result<orders::Order, AppError> {
    let id = Uuid::new_v4();
    orders::issue_ticket(
        pool,
        total,
        id,
        &input(n),
        slip_ref,
        &format!("orders/{id}.png"),
    )
    .await
}

async fn open(pool: &PgPool) {
    settings::set_sales_open(pool, true).await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn first_ticket_is_1(pool: PgPool) {
    open(&pool).await;
    let o = issue(&pool, 600, 1, "R1").await.unwrap();
    assert_eq!(o.ticket_no, 1);
    assert_eq!(o.status, "active");
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_50_distinct_numbers(pool: PgPool) {
    open(&pool).await;
    let mut handles = Vec::new();
    for n in 0..50u32 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            issue(&pool, 600, n, &format!("R{n}")).await
        }));
    }
    let mut nums = Vec::new();
    for h in handles {
        nums.push(h.await.unwrap().unwrap().ticket_no);
    }
    nums.sort();
    assert_eq!(nums, (1..=50).collect::<Vec<i16>>());
}

#[sqlx::test(migrations = "./migrations")]
async fn same_student_concurrent_one_ticket(pool: PgPool) {
    open(&pool).await;
    let (a, b) = tokio::join!(issue(&pool, 600, 7, "RA"), issue(&pool, 600, 7, "RB"));
    let results = [a, b];
    let ok = results.iter().filter(|r| r.is_ok()).count();
    let dup = results
        .iter()
        .filter(|r| matches!(r, Err(AppError::AlreadyHasTicket { ticket_no: 1, .. })))
        .count();
    assert_eq!((ok, dup), (1, 1));
}

#[sqlx::test(migrations = "./migrations")]
async fn cancelled_number_reused(pool: PgPool) {
    open(&pool).await;
    let mut third = None;
    for n in 1..=5 {
        let o = issue(&pool, 600, n, &format!("R{n}")).await.unwrap();
        if o.ticket_no == 3 {
            third = Some(o.id);
        }
    }
    orders::cancel_order(&pool, third.unwrap(), "สลิปปลอม")
        .await
        .unwrap();
    assert_eq!(issue(&pool, 600, 6, "R6").await.unwrap().ticket_no, 3);
}

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_slip_rejected_even_after_cancel(pool: PgPool) {
    open(&pool).await;
    let o = issue(&pool, 600, 1, "A").await.unwrap();
    orders::cancel_order(&pool, o.id, "x").await.unwrap();
    assert!(matches!(
        issue(&pool, 600, 2, "A").await,
        Err(AppError::DuplicateSlip)
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn student_can_rebuy_after_cancel(pool: PgPool) {
    open(&pool).await;
    let o = issue(&pool, 600, 1, "A").await.unwrap();
    orders::cancel_order(&pool, o.id, "x").await.unwrap();
    assert!(issue(&pool, 600, 1, "B").await.is_ok());
}

#[sqlx::test(migrations = "./migrations")]
async fn sold_out_when_total_reached(pool: PgPool) {
    open(&pool).await;
    for n in 1..=3 {
        issue(&pool, 3, n, &format!("R{n}")).await.unwrap();
    }
    assert!(matches!(
        issue(&pool, 3, 4, "R4").await,
        Err(AppError::SoldOut)
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn sales_closed_rejected(pool: PgPool) {
    assert!(matches!(
        issue(&pool, 600, 1, "R1").await,
        Err(AppError::SalesClosed)
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn close_forever_blocks_reopen(pool: PgPool) {
    open(&pool).await;
    let s = settings::close_forever(&pool).await.unwrap();
    assert!(!s.open && s.closed_forever);
    assert!(matches!(
        settings::set_sales_open(&pool, true).await,
        Err(AppError::PresaleClosedForever)
    ));
    let s = settings::get_sales_state(&pool).await.unwrap();
    assert!(!s.open && s.closed_forever);
    assert!(matches!(
        issue(&pool, 600, 1, "R1").await,
        Err(AppError::SalesClosed)
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn stats_free_numbers(pool: PgPool) {
    open(&pool).await;
    let mut second = None;
    for n in 1..=3 {
        let o = issue(&pool, 5, n, &format!("R{n}")).await.unwrap();
        if o.ticket_no == 2 {
            second = Some(o.id);
        }
    }
    orders::cancel_order(&pool, second.unwrap(), "x")
        .await
        .unwrap();
    let st = orders::stats(&pool, 5).await.unwrap();
    assert_eq!(
        (st.sold, st.remaining, st.cancelled, st.picked_up),
        (2, 3, 1, 0)
    );
    assert_eq!(st.free_numbers, vec![2, 4, 5]);
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_twice_conflict_and_pickup(pool: PgPool) {
    open(&pool).await;
    let o = issue(&pool, 600, 1, "R1").await.unwrap();
    let o2 = orders::set_picked_up(&pool, o.id, true).await.unwrap();
    assert!(o2.picked_up_at.is_some());
    orders::cancel_order(&pool, o.id, "x").await.unwrap();
    assert!(matches!(
        orders::cancel_order(&pool, o.id, "x").await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        orders::set_picked_up(&pool, o.id, false).await,
        Err(AppError::Conflict(_))
    ));
    assert!(matches!(
        orders::get_order(&pool, Uuid::new_v4()).await,
        Err(AppError::NotFound)
    ));
}

#[sqlx::test(migrations = "./migrations")]
async fn list_and_find(pool: PgPool) {
    open(&pool).await;
    for n in 1..=3 {
        issue(&pool, 600, n, &format!("R{n}")).await.unwrap();
    }
    assert_eq!(
        orders::list_orders(&pool, None, None).await.unwrap().len(),
        3
    );
    let by_no = orders::list_orders(&pool, Some("2"), None).await.unwrap();
    assert!(by_no.iter().any(|o| o.ticket_no == 2));
    let by_name = orders::list_orders(&pool, Some("คนที่ 3"), None)
        .await
        .unwrap();
    assert_eq!(by_name.len(), 1);
    let found = orders::find_active_by_student(&pool, "6800000001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.ticket_no, 1);
    assert!(
        orders::find_active_by_student(&pool, "6899999999")
            .await
            .unwrap()
            .is_none()
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn rejected_roundtrip(pool: PgPool) {
    let id = rejected::insert(
        &pool,
        "invalid_slip",
        Some("A"),
        Some("6800000001"),
        None,
        Some("rejected/x.png"),
        None,
    )
    .await
    .unwrap();
    let all = rejected::list(&pool).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(
        rejected::get(&pool, id).await.unwrap().reason,
        "invalid_slip"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn search_with_nul_does_not_error(pool: PgPool) {
    assert!(orders::list_orders(&pool, Some("a\0b"), None).await.is_ok());
}
