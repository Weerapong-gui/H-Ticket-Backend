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
async fn first_order_is_active(pool: PgPool) {
    open(&pool).await;
    let o = issue(&pool, 600, 1, "R1").await.unwrap();
    assert_eq!(o.status, "active");
    assert!(o.picked_up_at.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn concurrent_never_oversells(pool: PgPool) {
    open(&pool).await;
    let mut handles = Vec::new();
    for n in 0..50u32 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            issue(&pool, 30, n, &format!("R{n}")).await
        }));
    }
    let (mut ok, mut sold_out) = (0, 0);
    for h in handles {
        match h.await.unwrap() {
            Ok(_) => ok += 1,
            Err(AppError::SoldOut) => sold_out += 1,
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!((ok, sold_out), (30, 20));
}

#[sqlx::test(migrations = "./migrations")]
async fn same_student_concurrent_one_ticket(pool: PgPool) {
    open(&pool).await;
    let (a, b) = tokio::join!(issue(&pool, 600, 7, "RA"), issue(&pool, 600, 7, "RB"));
    let results = [a, b];
    let ok = results.iter().filter(|r| r.is_ok()).count();
    let dup = results
        .iter()
        .filter(|r| matches!(r, Err(AppError::AlreadyHasTicket { .. })))
        .count();
    assert_eq!((ok, dup), (1, 1));
}

#[sqlx::test(migrations = "./migrations")]
async fn cancel_frees_a_seat(pool: PgPool) {
    open(&pool).await;
    let mut first = None;
    for n in 1..=3 {
        let o = issue(&pool, 3, n, &format!("R{n}")).await.unwrap();
        first.get_or_insert(o.id);
    }
    assert!(matches!(
        issue(&pool, 3, 4, "R4").await,
        Err(AppError::SoldOut)
    ));
    orders::cancel_order(&pool, first.unwrap(), "สลิปปลอม")
        .await
        .unwrap();
    assert!(issue(&pool, 3, 5, "R5").await.is_ok());
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
async fn stats_counts(pool: PgPool) {
    open(&pool).await;
    let mut ids = Vec::new();
    for n in 1..=3 {
        ids.push(issue(&pool, 5, n, &format!("R{n}")).await.unwrap().id);
    }
    orders::cancel_order(&pool, ids[1], "x").await.unwrap();
    orders::set_picked_up(&pool, ids[0], true).await.unwrap();
    let st = orders::stats(&pool, 5).await.unwrap();
    assert_eq!(
        (st.sold, st.remaining, st.cancelled, st.picked_up),
        (2, 3, 1, 1)
    );
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
    let by_sid = orders::list_orders(&pool, Some("6800000002"), None)
        .await
        .unwrap();
    assert_eq!(by_sid.len(), 1);
    let by_name = orders::list_orders(&pool, Some("คนที่ 3"), None)
        .await
        .unwrap();
    assert_eq!(by_name.len(), 1);
    let found = orders::find_active_by_student(&pool, "6800000001")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(found.name, "คนที่ 1");
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

#[sqlx::test(migrations = "./migrations")]
async fn ghost_shots_count_up(pool: PgPool) {
    assert_eq!(settings::ghost_shots(&pool).await.unwrap(), 0);
    for _ in 0..3 {
        settings::add_ghost_shot(&pool).await.unwrap();
    }
    assert_eq!(settings::ghost_shots(&pool).await.unwrap(), 3);
}
