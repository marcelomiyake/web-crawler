use std::net::SocketAddr;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use crawler_domain::{MAX_DEPTH, MAX_URLS_PER_JOB, normalize_host, normalize_url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::net::TcpListener;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::{error, info};
use uuid::Uuid;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../database/postgres/migrations");

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateJobRequest {
    seeds: Vec<String>,
    allowed_hosts: Vec<String>,
    #[serde(default)]
    limits: Option<RequestedLimits>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RequestedLimits {
    max_urls: Option<i32>,
    max_depth: Option<i32>,
}

#[derive(Debug, Serialize)]
struct CreateJobResponse {
    id: Uuid,
    status: String,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct JobResponse {
    id: Uuid,
    status: String,
    max_urls: i32,
    max_depth: i32,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    counts: FrontierCounts,
}

#[derive(Debug, Default, Serialize)]
struct FrontierCounts {
    queued: i64,
    leased: i64,
    completed: i64,
    skipped: i64,
    failed: i64,
}

#[derive(Debug, Serialize)]
struct PageSummary {
    id: Uuid,
    canonical_url: String,
    title: String,
    http_status: i32,
    depth: i32,
    fetched_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct PageListResponse {
    pages: Vec<PageSummary>,
    next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PageQuery {
    cursor: Option<String>,
    limit: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize)]
struct PageCursor {
    fetched_at: DateTime<Utc>,
    id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct PageRow {
    id: Uuid,
    canonical_url: String,
    title: String,
    http_status: i32,
    depth: i32,
    fetched_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
    request_id: Uuid,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    category: &'static str,
    request_id: Uuid,
}

impl ApiError {
    fn new(status: StatusCode, category: &'static str) -> Self {
        Self {
            status,
            category,
            request_id: Uuid::new_v4(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.category,
                request_id: self.request_id,
            }),
        )
            .into_response()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let options = crawler_config::postgres_options()?;
    let pool = PgPoolOptions::new()
        .max_connections(12)
        .acquire_timeout(crawler_config::database_connect_timeout())
        .connect_with(options)
        .await?;
    MIGRATOR.run(&pool).await?;

    let app = build_app(pool);
    let address: SocketAddr = std::env::var("LISTEN_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8081".to_owned())
        .parse()?;
    let listener = TcpListener::bind(address).await?;
    info!("crawler API listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

fn build_app(pool: PgPool) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/crawl-jobs", post(create_job))
        .route(
            "/api/v1/crawl-jobs/{job_id}",
            get(get_job).delete(delete_job),
        )
        .route("/api/v1/crawl-jobs/{job_id}/pages", get(list_pages))
        .route(
            "/api/v1/crawl-jobs/{job_id}/pages/{page_id}/content",
            get(get_page_content),
        )
        .layer(RequestBodyLimitLayer::new(64 * 1024))
        .with_state(AppState { pool })
}

async fn live() -> StatusCode {
    StatusCode::NO_CONTENT
}

async fn ready(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&state.pool)
        .await
        .map_err(|_| ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "database_unavailable"))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn create_job(
    State(state): State<AppState>,
    headers: HeaderMap,
    payload: Result<Json<CreateJobRequest>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        [(header::HeaderName, HeaderValue); 1],
        Json<CreateJobResponse>,
    ),
    ApiError,
> {
    let Json(request) =
        payload.map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_json"))?;
    let key = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| {
            !value.is_empty() && value.len() <= 128 && value.bytes().all(|c| c.is_ascii_graphic())
        })
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "idempotency_key_required"))?
        .to_owned();
    let (seeds, allowed_hosts, max_urls, max_depth) = validate_create_request(request)?;
    let fingerprint = request_fingerprint(&seeds, &allowed_hosts, max_urls, max_depth);
    let mut tx = state.pool.begin().await.map_err(database_error)?;

    let created_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO crawl_jobs (id, idempotency_key, request_fingerprint, status, max_urls, max_depth) \
         VALUES ($1, $2, $3, 'running', $4, $5) ON CONFLICT (idempotency_key) DO NOTHING RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(&key)
    .bind(&fingerprint)
    .bind(max_urls)
    .bind(max_depth)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?;

    let job_id = if let Some(id) = created_id {
        let body_budget = sqlx::query_as::<_, (i64, i64)>(
            "SELECT used_body_bytes, max_body_bytes FROM archive_budget WHERE singleton = TRUE FOR SHARE",
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
        if body_budget.0 >= body_budget.1 {
            return Err(ApiError::new(
                StatusCode::INSUFFICIENT_STORAGE,
                "archive_quota_reached",
            ));
        }
        for host in &allowed_hosts {
            sqlx::query("INSERT INTO crawl_job_hosts (job_id, hostname) VALUES ($1, $2)")
                .bind(id)
                .bind(host)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
            sqlx::query(
                "INSERT INTO host_state (hostname) VALUES ($1) ON CONFLICT (hostname) DO NOTHING",
            )
            .bind(host)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        }
        for seed in &seeds {
            sqlx::query(
                "INSERT INTO frontier_urls (job_id, canonical_url, hostname, depth) VALUES ($1, $2, $3, 0)",
            )
            .bind(id)
            .bind(&seed.canonical)
            .bind(&seed.host)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        }
        id
    } else {
        let existing = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, request_fingerprint FROM crawl_jobs WHERE idempotency_key = $1",
        )
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or_else(|| ApiError::new(StatusCode::CONFLICT, "idempotency_conflict"))?;
        if existing.1.trim() != fingerprint {
            return Err(ApiError::new(StatusCode::CONFLICT, "idempotency_conflict"));
        }
        existing.0
    };
    tx.commit().await.map_err(database_error)?;

    let created_at =
        sqlx::query_scalar::<_, DateTime<Utc>>("SELECT created_at FROM crawl_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_one(&state.pool)
            .await
            .map_err(database_error)?;
    let response = CreateJobResponse {
        id: job_id,
        status: "running".to_owned(),
        created_at,
    };
    let location = HeaderValue::from_str(&format!("/api/v1/crawl-jobs/{job_id}"))
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok((
        StatusCode::ACCEPTED,
        [(header::LOCATION, location)],
        Json(response),
    ))
}

fn validate_create_request(
    request: CreateJobRequest,
) -> Result<(Vec<crawler_domain::NormalizedUrl>, Vec<String>, i32, i32), ApiError> {
    if request.seeds.is_empty()
        || request.seeds.len() > 20
        || request.allowed_hosts.is_empty()
        || request.allowed_hosts.len() > 20
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_job_size",
        ));
    }
    let mut allowed_hosts = request
        .allowed_hosts
        .iter()
        .map(|host| {
            normalize_host(host).map_err(|_| {
                ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_allowed_host")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    allowed_hosts.sort();
    allowed_hosts.dedup();
    let allowed = allowed_hosts
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let mut seeds = request
        .seeds
        .iter()
        .map(|seed| {
            normalize_url(seed)
                .map_err(|_| ApiError::new(StatusCode::UNPROCESSABLE_ENTITY, "invalid_seed_url"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    seeds.sort_by(|left, right| left.canonical.cmp(&right.canonical));
    seeds.dedup_by(|left, right| left.canonical == right.canonical);
    if seeds.iter().any(|seed| !allowed.contains(&seed.host)) {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "seed_host_not_allowed",
        ));
    }
    let max_urls = request
        .limits
        .as_ref()
        .and_then(|limits| limits.max_urls)
        .unwrap_or(MAX_URLS_PER_JOB as i32);
    let max_depth = request
        .limits
        .as_ref()
        .and_then(|limits| limits.max_depth)
        .unwrap_or(MAX_DEPTH);
    if max_urls < seeds.len() as i32
        || max_urls > MAX_URLS_PER_JOB as i32
        || !(0..=MAX_DEPTH).contains(&max_depth)
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid_job_limits",
        ));
    }
    Ok((seeds, allowed_hosts, max_urls, max_depth))
}

fn request_fingerprint(
    seeds: &[crawler_domain::NormalizedUrl],
    hosts: &[String],
    max_urls: i32,
    max_depth: i32,
) -> String {
    let payload = serde_json::json!({
        "seeds": seeds.iter().map(|seed| &seed.canonical).collect::<Vec<_>>(),
        "allowed_hosts": hosts,
        "max_urls": max_urls,
        "max_depth": max_depth
    });
    format!("{:x}", Sha256::digest(payload.to_string().as_bytes()))
}

async fn get_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<JobResponse>, ApiError> {
    let job = sqlx::query_as::<_, (String, i32, i32, DateTime<Utc>, Option<DateTime<Utc>>)>(
        "SELECT status, max_urls, max_depth, created_at, completed_at FROM crawl_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(database_error)?
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "job_not_found"))?;
    let counts = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        "SELECT count(*) FILTER (WHERE state = 'queued'), count(*) FILTER (WHERE state = 'leased'), \
         count(*) FILTER (WHERE state = 'completed'), count(*) FILTER (WHERE state = 'skipped'), \
         count(*) FILTER (WHERE state = 'failed') FROM frontier_urls WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_one(&state.pool)
    .await
    .map_err(database_error)?;
    Ok(Json(JobResponse {
        id: job_id,
        status: job.0,
        max_urls: job.1,
        max_depth: job.2,
        created_at: job.3,
        completed_at: job.4,
        counts: FrontierCounts {
            queued: counts.0,
            leased: counts.1,
            completed: counts.2,
            skipped: counts.3,
            failed: counts.4,
        },
    }))
}

async fn list_pages(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> Result<Json<PageListResponse>, ApiError> {
    if !job_exists(&state.pool, job_id).await? {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "job_not_found"));
    }
    let cursor = query.cursor.as_deref().map(decode_cursor).transpose()?;
    if query.limit.is_some_and(|limit| !(1..=100).contains(&limit)) {
        return Err(ApiError::new(StatusCode::BAD_REQUEST, "invalid_page_limit"));
    }
    let limit = query.limit.unwrap_or(50);
    let mut rows = sqlx::query_as::<_, PageRow>(
        "SELECT id, canonical_url, title, http_status, depth, fetched_at FROM page_snapshots \
         WHERE job_id = $1 AND ($2::timestamptz IS NULL OR (fetched_at, id) < ($2, $3)) \
         ORDER BY fetched_at DESC, id DESC LIMIT $4",
    )
    .bind(job_id)
    .bind(cursor.as_ref().map(|value| value.fetched_at))
    .bind(cursor.as_ref().map(|value| value.id))
    .bind(limit + 1)
    .fetch_all(&state.pool)
    .await
    .map_err(database_error)?;
    let has_more = rows.len() as i64 > limit;
    if has_more {
        rows.truncate(limit as usize);
    }
    let next_cursor = if has_more {
        rows.last().map(|row| {
            encode_cursor(&PageCursor {
                fetched_at: row.fetched_at,
                id: row.id,
            })
        })
    } else {
        None
    };
    Ok(Json(PageListResponse {
        pages: rows
            .into_iter()
            .map(|row| PageSummary {
                id: row.id,
                canonical_url: row.canonical_url,
                title: row.title,
                http_status: row.http_status,
                depth: row.depth,
                fetched_at: row.fetched_at,
            })
            .collect(),
        next_cursor,
    }))
}

async fn get_page_content(
    State(state): State<AppState>,
    Path((job_id, page_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiError> {
    let body = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT body FROM page_snapshots WHERE job_id = $1 AND id = $2",
    )
    .bind(job_id)
    .bind(page_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(database_error)?
    .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "page_not_found"))?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(Bytes::from(body)))
        .map_err(|_| ApiError::new(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))
}

async fn delete_job(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    let mut tx = state.pool.begin().await.map_err(database_error)?;
    let exists =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM crawl_jobs WHERE id = $1 FOR UPDATE")
            .bind(job_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(database_error)?
            .is_some();
    if !exists {
        return Err(ApiError::new(StatusCode::NOT_FOUND, "job_not_found"));
    }
    let bytes = sqlx::query_scalar::<_, i64>(
        "SELECT coalesce(sum(octet_length(body)), 0)::bigint FROM page_snapshots WHERE job_id = $1",
    )
    .bind(job_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(database_error)?;
    sqlx::query("UPDATE archive_budget SET used_body_bytes = greatest(0, used_body_bytes - $1), updated_at = now() WHERE singleton = TRUE")
        .bind(bytes)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    sqlx::query("DELETE FROM crawl_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    tx.commit().await.map_err(database_error)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn job_exists(pool: &PgPool, job_id: Uuid) -> Result<bool, ApiError> {
    sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM crawl_jobs WHERE id = $1)")
        .bind(job_id)
        .fetch_one(pool)
        .await
        .map_err(database_error)
}

fn encode_cursor(cursor: &PageCursor) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor).unwrap_or_default())
}

fn decode_cursor(cursor: &str) -> Result<PageCursor, ApiError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_cursor"))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| ApiError::new(StatusCode::BAD_REQUEST, "invalid_cursor"))
}

fn database_error(_error: sqlx::Error) -> ApiError {
    error!("database operation failed");
    ApiError::new(StatusCode::SERVICE_UNAVAILABLE, "database_unavailable")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        let _ = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("register SIGTERM")
            .recv()
            .await;
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = terminate => {} }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::to_bytes, http::Request};
    use sqlx::postgres::PgPoolOptions;
    use tower::ServiceExt;

    fn request(seeds: &[&str], hosts: &[&str]) -> CreateJobRequest {
        CreateJobRequest {
            seeds: seeds.iter().map(|s| (*s).to_owned()).collect(),
            allowed_hosts: hosts.iter().map(|s| (*s).to_owned()).collect(),
            limits: None,
        }
    }

    async fn test_pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("set TEST_DATABASE_URL to an isolated disposable PostgreSQL database");
        let pool = PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("connect to disposable PostgreSQL");
        MIGRATOR
            .run(&pool)
            .await
            .expect("apply PostgreSQL migrations");
        pool
    }

    async fn send(app: &Router, method: &str, uri: &str, body: &str) -> Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn send_with_key(app: &Router, body: &str, key: &str) -> Response {
        app.clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/crawl-jobs")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("idempotency-key", key)
                    .body(Body::from(body.to_owned()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    async fn response_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn create_body(seeds: &[&str]) -> String {
        serde_json::json!({
            "seeds": seeds,
            "allowed_hosts": ["example.org"]
        })
        .to_string()
    }

    #[tokio::test]
    async fn api_routes_cover_job_lifecycle_pages_cursor_and_content() {
        let pool = test_pool().await;
        let app = build_app(pool.clone());

        assert_eq!(
            send(&app, "GET", "/health/live", "").await.status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            send(&app, "GET", "/health/ready", "").await.status(),
            StatusCode::NO_CONTENT
        );

        let key = format!("api-lifecycle-{}", Uuid::new_v4());
        let body = create_body(&["https://example.org/one", "https://example.org/two"]);
        let created = send_with_key(&app, &body, &key).await;
        assert_eq!(created.status(), StatusCode::ACCEPTED);
        assert!(created.headers().contains_key(header::LOCATION));
        let created_json = response_json(created).await;
        let job_id = created_json["id"].as_str().unwrap();

        let repeated = response_json(send_with_key(&app, &body, &key).await).await;
        assert_eq!(repeated["id"], job_id);
        assert_eq!(
            send_with_key(&app, &create_body(&["https://example.org/other"]), &key)
                .await
                .status(),
            StatusCode::CONFLICT
        );

        let status =
            response_json(send(&app, "GET", &format!("/api/v1/crawl-jobs/{job_id}"), "").await)
                .await;
        assert_eq!(status["status"], "running");
        assert_eq!(status["counts"]["queued"], 2);
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/crawl-jobs/{}", Uuid::new_v4()),
                ""
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );

        let frontier = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, canonical_url FROM frontier_urls WHERE job_id = $1 ORDER BY canonical_url",
        )
        .bind(Uuid::parse_str(job_id).unwrap())
        .fetch_all(&pool)
        .await
        .unwrap();
        for (frontier_id, canonical_url) in &frontier {
            sqlx::query("INSERT INTO page_snapshots (job_id, frontier_url_id, canonical_url, final_url, hostname, title, http_status, depth, content_type, body, content_sha256) VALUES ($1, $2, $3, $3, 'example.org', 'Example', 200, 0, 'text/html', $4, $5)")
                .bind(Uuid::parse_str(job_id).unwrap())
                .bind(frontier_id)
                .bind(canonical_url)
                .bind(b"<html>archived</html>".as_slice())
                .bind("a".repeat(64))
                .execute(&pool)
                .await
                .unwrap();
        }
        sqlx::query("UPDATE archive_budget SET used_body_bytes = used_body_bytes + 42 WHERE singleton = TRUE")
            .execute(&pool)
            .await
            .unwrap();

        let first = send(
            &app,
            "GET",
            &format!("/api/v1/crawl-jobs/{job_id}/pages?limit=1"),
            "",
        )
        .await;
        assert_eq!(first.status(), StatusCode::OK);
        let first_json = response_json(first).await;
        let page_id = first_json["pages"][0]["id"].as_str().unwrap();
        let cursor = first_json["next_cursor"].as_str().unwrap();
        let second = response_json(
            send(
                &app,
                "GET",
                &format!("/api/v1/crawl-jobs/{job_id}/pages?limit=1&cursor={cursor}"),
                "",
            )
            .await,
        )
        .await;
        assert_eq!(second["pages"].as_array().unwrap().len(), 1);
        assert!(second["next_cursor"].is_null());

        let content = send(
            &app,
            "GET",
            &format!("/api/v1/crawl-jobs/{job_id}/pages/{page_id}/content"),
            "",
        )
        .await;
        assert_eq!(content.status(), StatusCode::OK);
        assert_eq!(
            content.headers()[header::CONTENT_TYPE],
            "text/plain; charset=utf-8"
        );
        assert_eq!(content.headers()["x-content-type-options"], "nosniff");
        assert_eq!(
            to_bytes(content.into_body(), usize::MAX).await.unwrap(),
            "<html>archived</html>"
        );

        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/crawl-jobs/{job_id}/pages?limit=0"),
                ""
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/crawl-jobs/{job_id}/pages?cursor=%25bad"),
                ""
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            send(
                &app,
                "GET",
                &format!(
                    "/api/v1/crawl-jobs/{job_id}/pages/{}/content",
                    Uuid::new_v4()
                ),
                ""
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/crawl-jobs/{}/pages", Uuid::new_v4()),
                ""
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );

        assert_eq!(
            send(&app, "DELETE", &format!("/api/v1/crawl-jobs/{job_id}"), "")
                .await
                .status(),
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            send(&app, "GET", &format!("/api/v1/crawl-jobs/{job_id}"), "")
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            send(&app, "DELETE", &format!("/api/v1/crawl-jobs/{job_id}"), "")
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        let budget: i64 =
            sqlx::query_scalar("SELECT used_body_bytes FROM archive_budget WHERE singleton = TRUE")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(budget >= 0);
        pool.close().await;
    }

    #[tokio::test]
    async fn api_rejects_bad_json_idempotency_and_job_inputs() {
        let pool = test_pool().await;
        let app = build_app(pool.clone());
        let bad_json = send_with_key(&app, "{", "bad-json").await;
        assert_eq!(bad_json.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response_json(bad_json).await["error"], "invalid_json");

        let missing_key = send(
            &app,
            "POST",
            "/api/v1/crawl-jobs",
            &create_body(&["https://example.org/"]),
        )
        .await;
        assert_eq!(missing_key.status(), StatusCode::BAD_REQUEST);
        let invalid_key =
            send_with_key(&app, &create_body(&["https://example.org/"]), "bad key").await;
        assert_eq!(invalid_key.status(), StatusCode::BAD_REQUEST);
        let invalid_host = send_with_key(
            &app,
            &serde_json::json!({"seeds":["https://example.org/"],"allowed_hosts":["/bad"]})
                .to_string(),
            "bad-host",
        )
        .await;
        assert_eq!(invalid_host.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let too_many_seeds =
            serde_json::json!({"seeds":[],"allowed_hosts":["example.org"]}).to_string();
        assert_eq!(
            send_with_key(&app, &too_many_seeds, "empty-seeds")
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let disallowed_seed = serde_json::json!({"seeds":["https://outside.example/"],"allowed_hosts":["example.org"]}).to_string();
        assert_eq!(
            send_with_key(&app, &disallowed_seed, "outside-seed")
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        let invalid_limits = serde_json::json!({"seeds":["https://example.org/"],"allowed_hosts":["example.org"],"limits":{"max_depth":4}}).to_string();
        assert_eq!(
            send_with_key(&app, &invalid_limits, "bad-limits")
                .await
                .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            send_with_key(
                &app,
                &create_body(&["https://example.org/"]),
                &"x".repeat(129)
            )
            .await
            .status(),
            StatusCode::BAD_REQUEST
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn api_maps_closed_database_errors_to_service_unavailable() {
        let pool = test_pool().await;
        pool.close().await;
        let app = build_app(pool);
        let ready = send(&app, "GET", "/health/ready", "").await;
        assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response_json(ready).await["error"], "database_unavailable");
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/crawl-jobs/{}", Uuid::new_v4()),
                ""
            )
            .await
            .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }

    #[test]
    fn normalizes_and_deduplicates_seeds_and_allowlist_entries() {
        let (seeds, hosts, max_urls, max_depth) = validate_create_request(request(
            &["https://Example.org/#one", "https://example.org/#two"],
            &["EXAMPLE.ORG", "example.org"],
        ))
        .unwrap();
        assert_eq!(seeds.len(), 1);
        assert_eq!(hosts, vec!["example.org"]);
        assert_eq!(max_urls, 500);
        assert_eq!(max_depth, 3);
    }

    #[test]
    fn refuses_seed_outside_exact_host_allowlist() {
        let error =
            validate_create_request(request(&["https://sub.example.org"], &["example.org"]))
                .unwrap_err();
        assert_eq!(error.category, "seed_host_not_allowed");
    }

    #[test]
    fn idempotency_fingerprint_ignores_input_order_after_normalization() {
        let (a_seeds, a_hosts, _, _) = validate_create_request(request(
            &["https://b.example/", "https://a.example/"],
            &["b.example", "a.example"],
        ))
        .unwrap();
        let (b_seeds, b_hosts, _, _) = validate_create_request(request(
            &["https://a.example/", "https://b.example/"],
            &["a.example", "b.example"],
        ))
        .unwrap();
        assert_eq!(
            request_fingerprint(&a_seeds, &a_hosts, 500, 3),
            request_fingerprint(&b_seeds, &b_hosts, 500, 3)
        );
    }

    #[test]
    fn rejects_limits_beyond_hard_caps() {
        let mut request = request(&["https://example.org"], &["example.org"]);
        request.limits = Some(RequestedLimits {
            max_urls: Some(501),
            max_depth: Some(3),
        });
        assert_eq!(
            validate_create_request(request).unwrap_err().category,
            "invalid_job_limits"
        );
    }

    #[test]
    fn page_cursor_round_trips_and_rejects_garbage() {
        let cursor = PageCursor {
            fetched_at: Utc::now(),
            id: Uuid::new_v4(),
        };
        assert_eq!(
            decode_cursor(&encode_cursor(&cursor)).unwrap().id,
            cursor.id
        );
        assert_eq!(
            decode_cursor("%bad").unwrap_err().category,
            "invalid_cursor"
        );
    }
}
