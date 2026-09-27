use sqlx::{PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../database/postgres/migrations");

async fn disposable_test_database() -> PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("set TEST_DATABASE_URL to an isolated disposable PostgreSQL database");
    PgPoolOptions::new()
        .max_connections(4)
        .connect(&url)
        .await
        .expect("connect to the disposable PostgreSQL database")
}

#[tokio::test]
async fn migrations_uniqueness_claim_locks_and_cascade_are_consistent() {
    let pool = disposable_test_database().await;
    // The same migration process runs in every API replica during bootstrap.
    // Exercise concurrent startup against an empty disposable database.
    let (left, right) = tokio::join!(MIGRATOR.run(&pool), MIGRATOR.run(&pool));
    left.expect("first concurrent migration");
    right.expect("second concurrent migration");

    let job_id = Uuid::new_v4();
    let idempotency_key = format!("integration-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO crawl_jobs (id, idempotency_key, request_fingerprint, status, max_urls, max_depth) VALUES ($1, $2, $3, 'running', 10, 2)")
        .bind(job_id)
        .bind(&idempotency_key)
        .bind("a".repeat(64))
        .execute(&pool)
        .await
        .unwrap();

    let host_a = format!("a-{}.example.test", Uuid::new_v4().simple());
    let host_b = format!("b-{}.example.test", Uuid::new_v4().simple());
    for host in [&host_a, &host_b] {
        sqlx::query("INSERT INTO crawl_job_hosts (job_id, hostname) VALUES ($1, $2)")
            .bind(job_id)
            .bind(host)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO host_state (hostname) VALUES ($1)")
            .bind(host)
            .execute(&pool)
            .await
            .unwrap();
    }

    let first_url = format!("https://{host_a}/one");
    let first = sqlx::query_scalar::<_, Uuid>("INSERT INTO frontier_urls (job_id, canonical_url, hostname, depth) VALUES ($1, $2, $3, 0) RETURNING id")
        .bind(job_id).bind(&first_url).bind(&host_a).fetch_one(&pool).await.unwrap();
    let duplicate = sqlx::query_scalar::<_, Uuid>("INSERT INTO frontier_urls (job_id, canonical_url, hostname, depth) VALUES ($1, $2, $3, 0) ON CONFLICT (job_id, canonical_url) DO NOTHING RETURNING id")
        .bind(job_id).bind(&first_url).bind(&host_a).fetch_optional(&pool).await.unwrap();
    assert!(
        duplicate.is_none(),
        "normalized URL identity is unique within a job"
    );

    let second_url = format!("https://{host_b}/two");
    let second = sqlx::query_scalar::<_, Uuid>("INSERT INTO frontier_urls (job_id, canonical_url, hostname, depth) VALUES ($1, $2, $3, 0) RETURNING id")
        .bind(job_id).bind(&second_url).bind(&host_b).fetch_one(&pool).await.unwrap();

    let mut first_claim = pool.begin().await.unwrap();
    let locked = sqlx::query_scalar::<_, Uuid>(
        "SELECT f.id FROM frontier_urls f JOIN host_state h ON h.hostname = f.hostname \
         WHERE f.job_id = $1 AND f.state = 'queued' ORDER BY f.hostname \
         FOR UPDATE OF f, h SKIP LOCKED LIMIT 1",
    )
    .bind(job_id)
    .fetch_one(&mut *first_claim)
    .await
    .unwrap();
    let mut second_claim = pool.begin().await.unwrap();
    let unlocked = sqlx::query_scalar::<_, Uuid>(
        "SELECT f.id FROM frontier_urls f JOIN host_state h ON h.hostname = f.hostname \
         WHERE f.job_id = $1 AND f.state = 'queued' ORDER BY f.hostname \
         FOR UPDATE OF f, h SKIP LOCKED LIMIT 1",
    )
    .bind(job_id)
    .fetch_one(&mut *second_claim)
    .await
    .unwrap();
    assert_ne!(
        locked, unlocked,
        "independent transactions claim distinct host work"
    );
    first_claim.commit().await.unwrap();
    second_claim.commit().await.unwrap();
    assert_eq!(
        HashSet::from([locked, unlocked]),
        HashSet::from([first, second])
    );

    sqlx::query("DELETE FROM crawl_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .unwrap();
    let remaining =
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM frontier_urls WHERE job_id = $1")
            .bind(job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        remaining, 0,
        "job deletion cascades to durable frontier rows"
    );
    sqlx::query("DELETE FROM host_state WHERE hostname = ANY($1)")
        .bind(vec![host_a, host_b])
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

use std::collections::HashSet;
