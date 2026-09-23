mod robots;

use std::{collections::HashSet, net::SocketAddr, time::Duration};

use anyhow::{Context, Result};
use axum::{Router, http::StatusCode, routing::get};
use chrono::{DateTime, Utc};
use crawler_domain::{
    MAX_ATTEMPTS, MAX_LINK_CANDIDATES, MAX_REDIRECTS, MAX_RESPONSE_BYTES, is_public_destination,
    normalize_url,
};
use futures_util::StreamExt;
use scraper::{Html, Selector};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction, postgres::PgPoolOptions};
use tokio::{
    net::{TcpListener, lookup_host},
    time::{Instant, sleep, timeout},
};
use tracing::{error, info, warn};
use url::Url;
use uuid::Uuid;

const WORK_LEASE_SECONDS: i64 = 45;
const ROBOTS_BODY_BYTES: usize = 512 * 1024;
const ROBOTS_TTL_SECONDS: i64 = 3600;
const ROBOTS_RETRY_SECONDS: i64 = 30;
const REQUEST_TIMEOUT_SECONDS: u64 = 10;
const MIN_HOST_DELAY_MS: i64 = 1_000;
const ARCHIVE_QUOTA_ERROR: &str = "archive_quota";

#[cfg(test)]
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("../database/postgres/migrations");

#[derive(Clone)]
struct WorkerState {
    pool: PgPool,
    user_agent: String,
    product_token: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct WorkClaim {
    frontier_id: Uuid,
    job_id: Uuid,
    canonical_url: String,
    hostname: String,
    depth: i32,
    max_depth: i32,
    max_urls: i32,
    attempts: i32,
    lease_token: Uuid,
}

#[derive(Debug)]
struct FetchFailure {
    category: &'static str,
    retryable: bool,
    retry_after_seconds: Option<u64>,
}

impl FetchFailure {
    fn permanent(category: &'static str) -> Self {
        Self {
            category,
            retryable: false,
            retry_after_seconds: None,
        }
    }
    fn retryable(category: &'static str) -> Self {
        Self {
            category,
            retryable: true,
            retry_after_seconds: None,
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct RobotsRow {
    outcome: String,
    rules: String,
    expires_at: DateTime<Utc>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let user_agent =
        std::env::var("CRAWLER_USER_AGENT").unwrap_or_else(|_| "WebCrawler/0.1".to_owned());
    let product_token =
        std::env::var("CRAWLER_PRODUCT_TOKEN").unwrap_or_else(|_| "WebCrawler/0.1".to_owned());
    if user_agent.len() > 200
        || user_agent.contains(['\r', '\n'])
        || product_token.is_empty()
        || product_token.len() > 64
        || product_token.contains(['\r', '\n'])
    {
        anyhow::bail!("crawler user-agent configuration is invalid");
    }
    let options = crawler_config::postgres_options()?;
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .acquire_timeout(crawler_config::database_connect_timeout())
        .connect_with(options)
        .await
        .context("connect to PostgreSQL")?;
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(&pool)
        .await?;
    let state = WorkerState {
        pool,
        user_agent,
        product_token,
    };
    let concurrency = std::env::var("WORKER_CONCURRENCY")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(2)
        .clamp(1, 8);
    info!(concurrency, "crawler worker started");
    let health_state = state.clone();
    let health_task = tokio::spawn(async move {
        let app = Router::new()
            .route("/health/live", get(|| async { StatusCode::NO_CONTENT }))
            .route(
                "/health/ready",
                get(move || {
                    let pool = health_state.pool.clone();
                    async move {
                        if sqlx::query_scalar::<_, i32>("SELECT 1")
                            .fetch_one(&pool)
                            .await
                            .is_ok()
                        {
                            StatusCode::NO_CONTENT
                        } else {
                            StatusCode::SERVICE_UNAVAILABLE
                        }
                    }
                }),
            );
        let listener = TcpListener::bind("0.0.0.0:8082").await?;
        axum::serve(listener, app).await?;
        Ok::<(), std::io::Error>(())
    });
    let mut tasks = tokio::task::JoinSet::new();
    for _ in 0..concurrency {
        let worker = state.clone();
        tasks.spawn(async move { worker_loop(worker).await });
    }
    tokio::select! {
        _ = tokio::signal::ctrl_c() => info!("crawler worker shutting down"),
        result = tasks.join_next() => {
            if let Some(Err(error)) = result { return Err(error.into()); }
        }
    }
    tasks.abort_all();
    health_task.abort();
    while tasks.join_next().await.is_some() {}
    Ok(())
}

async fn worker_loop(state: WorkerState) {
    loop {
        if let Err(_error) = reap_and_complete(&state.pool).await {
            error!(error_category = "database", "worker maintenance failed");
        }
        match claim_next(&state.pool).await {
            Ok(Some(claim)) => {
                if let Err(_error) = process_claim(&state, claim).await {
                    error!(
                        error_category = "claim_processing",
                        "worker could not finish claimed URL"
                    );
                }
            }
            Ok(None) => sleep(Duration::from_millis(500)).await,
            Err(_) => {
                error!(error_category = "database", "worker claim failed");
                sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn reap_and_complete(pool: &PgPool) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE frontier_urls SET state = 'failed', lease_token = NULL, lease_expires_at = NULL, \
         last_error = 'lease_expired' WHERE state = 'leased' AND lease_expires_at < now() AND attempts >= $1",
    )
    .bind(MAX_ATTEMPTS)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE frontier_urls SET state = 'queued', lease_token = NULL, lease_expires_at = NULL, \
         next_attempt_at = now() WHERE state = 'leased' AND lease_expires_at < now() AND attempts < $1",
    )
    .bind(MAX_ATTEMPTS)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE host_state SET lease_token = NULL, lease_expires_at = NULL WHERE lease_expires_at < now()")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE crawl_jobs j SET status = CASE WHEN EXISTS (SELECT 1 FROM frontier_urls f WHERE f.job_id = j.id AND f.state IN ('failed', 'skipped')) \
             THEN 'completed_with_errors' ELSE 'completed' END, completed_at = now() \
         WHERE j.status = 'running' AND NOT EXISTS (SELECT 1 FROM frontier_urls f WHERE f.job_id = j.id AND f.state IN ('queued', 'leased'))",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM host_state h WHERE h.lease_token IS NULL AND h.next_request_at <= now() AND NOT EXISTS (SELECT 1 FROM frontier_urls f WHERE f.hostname = h.hostname AND f.state IN ('queued', 'leased'))")
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM robots_cache WHERE expires_at <= now()")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

async fn claim_next(pool: &PgPool) -> Result<Option<WorkClaim>> {
    let mut tx = pool.begin().await?;
    let candidate = sqlx::query_as::<_, WorkClaim>(
        "SELECT f.id AS frontier_id, f.job_id, f.canonical_url, f.hostname, f.depth, \
                j.max_depth, j.max_urls, f.attempts, gen_random_uuid() AS lease_token \
         FROM frontier_urls f \
         JOIN crawl_jobs j ON j.id = f.job_id AND j.status = 'running' \
         JOIN crawl_job_hosts allowed ON allowed.job_id = f.job_id AND allowed.hostname = f.hostname \
         JOIN host_state h ON h.hostname = f.hostname \
         WHERE f.state = 'queued' AND f.next_attempt_at <= now() AND f.attempts < $1 \
           AND h.next_request_at <= now() AND (h.lease_expires_at IS NULL OR h.lease_expires_at < now()) \
         ORDER BY f.next_attempt_at, f.discovered_at \
         FOR UPDATE OF f, h SKIP LOCKED LIMIT 1",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(claim) = candidate {
        sqlx::query(
            "UPDATE frontier_urls SET state = 'leased', attempts = attempts + 1, lease_token = $2, \
             lease_expires_at = now() + ($3 * interval '1 second') WHERE id = $1",
        )
        .bind(claim.frontier_id)
        .bind(claim.lease_token)
        .bind(WORK_LEASE_SECONDS)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE host_state SET lease_token = $2, lease_expires_at = now() + ($3 * interval '1 second') WHERE hostname = $1",
        )
        .bind(&claim.hostname)
        .bind(claim.lease_token)
        .bind(WORK_LEASE_SECONDS)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Some(claim))
    } else {
        tx.rollback().await?;
        Ok(None)
    }
}

async fn process_claim(state: &WorkerState, claim: WorkClaim) -> Result<()> {
    let normalized = match normalize_url(&claim.canonical_url) {
        Ok(value) if value.host == claim.hostname => value,
        _ => {
            return finish_failure(
                state,
                &claim,
                FetchFailure::permanent("invalid_frontier_url"),
                0,
                MIN_HOST_DELAY_MS,
            )
            .await;
        }
    };
    match ensure_robots(state, &claim, &normalized.canonical).await {
        Ok(RobotsDecision::Deferred(until)) => {
            return defer_claim(state, &claim, until, true).await;
        }
        Ok(RobotsDecision::Disallowed) => {
            return finish_skipped(state, &claim, "robots_disallowed", None).await;
        }
        Ok(RobotsDecision::Allowed { delay_ms, rules }) => {
            set_host_next_request(&state.pool, &claim, delay_ms).await?;
            return process_page(state, &claim, &normalized.canonical, &rules, delay_ms).await;
        }
        Err(_) => {
            return defer_claim(
                state,
                &claim,
                Utc::now() + chrono::Duration::seconds(ROBOTS_RETRY_SECONDS),
                true,
            )
            .await;
        }
    }
}

async fn process_page(
    state: &WorkerState,
    claim: &WorkClaim,
    canonical_url: &str,
    robots_rules: &str,
    host_delay_ms: i64,
) -> Result<()> {
    let started = Instant::now();
    match timeout(
        Duration::from_secs(REQUEST_TIMEOUT_SECONDS),
        fetch_page(state, claim, canonical_url, robots_rules, host_delay_ms),
    )
    .await
    {
        Ok(Ok(page)) => save_page(state, claim, page, host_delay_ms).await,
        Ok(Err(failure)) => {
            finish_failure(
                state,
                claim,
                failure,
                started.elapsed().as_secs(),
                host_delay_ms,
            )
            .await
        }
        Err(_) => {
            finish_failure(
                state,
                claim,
                FetchFailure::retryable("fetch_timeout"),
                REQUEST_TIMEOUT_SECONDS,
                host_delay_ms,
            )
            .await
        }
    }
}

enum RobotsDecision {
    Allowed { delay_ms: i64, rules: String },
    Disallowed,
    Deferred(DateTime<Utc>),
}

async fn ensure_robots(
    state: &WorkerState,
    claim: &WorkClaim,
    page_url: &str,
) -> Result<RobotsDecision> {
    let url = Url::parse(page_url)?;
    let origin = url.origin().ascii_serialization();
    let cached = sqlx::query_as::<_, RobotsRow>(
        "SELECT outcome, rules, expires_at FROM robots_cache WHERE origin = $1 AND expires_at > now()",
    )
    .bind(&origin)
    .fetch_optional(&state.pool)
    .await?;
    if let Some(cached) = cached {
        return match cached.outcome.as_str() {
            "deny" => Ok(RobotsDecision::Disallowed),
            "defer" => Ok(RobotsDecision::Deferred(cached.expires_at)),
            "allow" => Ok(RobotsDecision::Allowed {
                delay_ms: MIN_HOST_DELAY_MS,
                rules: String::new(),
            }),
            _ => {
                let rules = robots::parse(&cached.rules, &state.product_token);
                if rules.allows(&url) {
                    Ok(RobotsDecision::Allowed {
                        delay_ms: rules.delay_ms,
                        rules: cached.rules,
                    })
                } else {
                    Ok(RobotsDecision::Disallowed)
                }
            }
        };
    }

    let robots_url = format!("{origin}/robots.txt");
    match timeout(
        Duration::from_secs(REQUEST_TIMEOUT_SECONDS),
        request_once(&robots_url, &state.user_agent, ROBOTS_BODY_BYTES),
    )
    .await
    {
        Ok(Ok(response)) if (200..300).contains(&response.status) => {
            let rules = String::from_utf8_lossy(&response.body).into_owned();
            let parsed = robots::parse(&rules, &state.product_token);
            set_host_next_request(&state.pool, claim, parsed.delay_ms).await?;
            sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'rules', $2, now() + ($3 * interval '1 second')) ON CONFLICT (origin) DO UPDATE SET outcome = 'rules', rules = EXCLUDED.rules, cached_at = now(), expires_at = EXCLUDED.expires_at")
                .bind(&origin).bind(&rules).bind(ROBOTS_TTL_SECONDS).execute(&state.pool).await?;
            Ok(RobotsDecision::Deferred(
                Utc::now() + chrono::Duration::milliseconds(parsed.delay_ms),
            ))
        }
        Ok(Ok(response)) if response.status == 404 || response.status == 410 => {
            sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'allow', '', now() + ($2 * interval '1 second')) ON CONFLICT (origin) DO UPDATE SET outcome = 'allow', rules = '', cached_at = now(), expires_at = EXCLUDED.expires_at")
                .bind(&origin).bind(ROBOTS_TTL_SECONDS).execute(&state.pool).await?;
            set_host_next_request(&state.pool, claim, MIN_HOST_DELAY_MS).await?;
            Ok(RobotsDecision::Deferred(
                Utc::now() + chrono::Duration::milliseconds(MIN_HOST_DELAY_MS),
            ))
        }
        Ok(Ok(response)) if response.status == 401 || response.status == 403 => {
            sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'deny', '', now() + ($2 * interval '1 second')) ON CONFLICT (origin) DO UPDATE SET outcome = 'deny', rules = '', cached_at = now(), expires_at = EXCLUDED.expires_at")
                .bind(&origin).bind(ROBOTS_TTL_SECONDS).execute(&state.pool).await?;
            Ok(RobotsDecision::Disallowed)
        }
        _ => {
            let until = Utc::now() + chrono::Duration::seconds(ROBOTS_RETRY_SECONDS);
            sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'defer', '', $2) ON CONFLICT (origin) DO UPDATE SET outcome = 'defer', rules = '', cached_at = now(), expires_at = EXCLUDED.expires_at")
                .bind(&origin).bind(until).execute(&state.pool).await?;
            Ok(RobotsDecision::Deferred(until))
        }
    }
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: reqwest::header::HeaderMap,
    body: Vec<u8>,
}

#[derive(Debug)]
struct PageFetch {
    status: u16,
    final_url: Url,
    content_type: String,
    body: Vec<u8>,
    title: String,
    discovered: Vec<crawler_domain::NormalizedUrl>,
}

async fn fetch_page(
    state: &WorkerState,
    claim: &WorkClaim,
    initial: &str,
    robots_rules: &str,
    host_delay_ms: i64,
) -> std::result::Result<PageFetch, FetchFailure> {
    let allowed_hosts = load_allowed_hosts(&state.pool, claim.job_id).await?;
    let initial_url =
        Url::parse(initial).map_err(|_| FetchFailure::permanent("invalid_frontier_url"))?;
    let (final_url, response) = fetch_final_response(
        state,
        claim,
        &initial_url,
        robots_rules,
        host_delay_ms,
        &allowed_hosts,
    )
    .await?;
    page_from_response(response, final_url, claim, &allowed_hosts)
}

async fn load_allowed_hosts(pool: &PgPool, job_id: Uuid) -> Result<HashSet<String>, FetchFailure> {
    sqlx::query_scalar::<_, String>("SELECT hostname FROM crawl_job_hosts WHERE job_id = $1")
        .bind(job_id)
        .fetch_all(pool)
        .await
        .map(|hosts| hosts.into_iter().collect())
        .map_err(|_| FetchFailure::retryable("database"))
}

async fn fetch_final_response(
    state: &WorkerState,
    claim: &WorkClaim,
    initial_url: &Url,
    robots_rules: &str,
    host_delay_ms: i64,
    allowed_hosts: &HashSet<String>,
) -> std::result::Result<(Url, HttpResponse), FetchFailure> {
    let mut current = initial_url.clone();
    for redirect_count in 0..=MAX_REDIRECTS {
        validate_fetch_url(
            &current,
            initial_url,
            claim,
            robots_rules,
            state,
            allowed_hosts,
        )?;
        let response =
            request_once(current.as_str(), &state.user_agent, MAX_RESPONSE_BYTES).await?;
        if let Some(failure) = retryable_http_failure(&response) {
            return Err(failure);
        }
        if !is_redirect_status(response.status) {
            return Ok((current, response));
        }
        current = redirect_target(&current, &response, claim, allowed_hosts, redirect_count)?;
        sleep(Duration::from_millis(
            host_delay_ms.max(MIN_HOST_DELAY_MS) as u64
        ))
        .await;
    }
    Err(FetchFailure::permanent("redirect_limit"))
}

fn validate_fetch_url(
    current: &Url,
    initial_url: &Url,
    claim: &WorkClaim,
    robots_rules: &str,
    state: &WorkerState,
    allowed_hosts: &HashSet<String>,
) -> std::result::Result<(), FetchFailure> {
    let normalized = normalize_url(current.as_str())
        .map_err(|_| FetchFailure::permanent("redirect_rejected"))?;
    if !allowed_hosts.contains(&normalized.host) || normalized.host != claim.hostname {
        return Err(FetchFailure::permanent("redirect_host_rejected"));
    }
    if current.origin().ascii_serialization() != initial_url.origin().ascii_serialization()
        || !robots::parse(robots_rules, &state.product_token).allows(current)
    {
        return Err(FetchFailure::permanent("redirect_robots_rejected"));
    }
    Ok(())
}

fn retryable_http_failure(response: &HttpResponse) -> Option<FetchFailure> {
    if response.status != 429 && response.status < 500 {
        return None;
    }
    let retry_after_seconds = response
        .headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|value| value.min(60));
    Some(FetchFailure {
        category: if response.status == 429 {
            "http_429"
        } else {
            "http_5xx"
        },
        retryable: true,
        retry_after_seconds,
    })
}

fn is_redirect_status(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

fn redirect_target(
    current: &Url,
    response: &HttpResponse,
    claim: &WorkClaim,
    allowed_hosts: &HashSet<String>,
    redirect_count: usize,
) -> std::result::Result<Url, FetchFailure> {
    if redirect_count >= MAX_REDIRECTS {
        return Err(FetchFailure::permanent("redirect_limit"));
    }
    let location = response
        .headers
        .get(reqwest::header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| FetchFailure::permanent("invalid_redirect"))?;
    let next = current
        .join(location)
        .map_err(|_| FetchFailure::permanent("invalid_redirect"))?;
    let normalized =
        normalize_url(next.as_str()).map_err(|_| FetchFailure::permanent("redirect_rejected"))?;
    if normalized.host != claim.hostname || !allowed_hosts.contains(&normalized.host) {
        return Err(FetchFailure::permanent("redirect_host_rejected"));
    }
    Url::parse(&normalized.canonical).map_err(|_| FetchFailure::permanent("invalid_redirect"))
}

fn page_from_response(
    response: HttpResponse,
    final_url: Url,
    claim: &WorkClaim,
    allowed_hosts: &HashSet<String>,
) -> std::result::Result<PageFetch, FetchFailure> {
    let content_type = response
        .headers
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if !matches!(content_type.as_str(), "text/html" | "application/xhtml+xml") {
        return Err(FetchFailure::permanent("non_html_content"));
    }
    let text = String::from_utf8_lossy(&response.body).into_owned();
    let document = Html::parse_document(&text);
    let title = page_title(&document)?;
    let discovered = discover_links(&document, &final_url, claim, allowed_hosts)?;
    Ok(PageFetch {
        status: response.status,
        final_url,
        content_type,
        body: response.body,
        title,
        discovered,
    })
}

fn page_title(document: &Html) -> std::result::Result<String, FetchFailure> {
    let selector = Selector::parse("title").map_err(|_| FetchFailure::permanent("html_parse"))?;
    Ok(document
        .select(&selector)
        .next()
        .map(|node| {
            node.text()
                .collect::<String>()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
        .chars()
        .take(512)
        .collect())
}

fn discover_links(
    document: &Html,
    current: &Url,
    claim: &WorkClaim,
    allowed_hosts: &HashSet<String>,
) -> std::result::Result<Vec<crawler_domain::NormalizedUrl>, FetchFailure> {
    if claim.depth >= claim.max_depth {
        return Ok(Vec::new());
    }
    let selector = Selector::parse("a[href]").map_err(|_| FetchFailure::permanent("html_parse"))?;
    let mut discovered = Vec::new();
    let mut seen = HashSet::new();
    for anchor in document.select(&selector).take(MAX_LINK_CANDIDATES) {
        let Some(href) = anchor.value().attr("href") else {
            continue;
        };
        let Ok(target) = current.join(href) else {
            continue;
        };
        let Ok(normalized) = normalize_url(target.as_str()) else {
            continue;
        };
        if allowed_hosts.contains(&normalized.host) && seen.insert(normalized.canonical.clone()) {
            discovered.push(normalized);
        }
    }
    Ok(discovered)
}

async fn request_once(
    url: &str,
    user_agent: &str,
    body_limit: usize,
) -> std::result::Result<HttpResponse, FetchFailure> {
    let parsed = Url::parse(url).map_err(|_| FetchFailure::permanent("invalid_url"))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| FetchFailure::permanent("invalid_host"))?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| FetchFailure::permanent("invalid_port"))?;
    let mut addresses = lookup_host((host, port))
        .await
        .map_err(|_| FetchFailure::retryable("dns_failure"))?
        .collect::<Vec<SocketAddr>>();
    addresses.sort();
    addresses.dedup();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !is_public_destination(address.ip()))
    {
        return Err(FetchFailure::permanent("non_public_destination"));
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECONDS))
        .user_agent(user_agent)
        .resolve_to_addrs(host, &addresses)
        .build()
        .map_err(|_| FetchFailure::permanent("http_client"))?;
    let response = client.get(parsed).send().await.map_err(|error| {
        if error.is_timeout() {
            FetchFailure::retryable("fetch_timeout")
        } else if error.is_connect() {
            FetchFailure::retryable("connect_failure")
        } else {
            FetchFailure::retryable("request_failure")
        }
    })?;
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let declared = response.content_length();
    if declared.is_some_and(|length| length > body_limit as u64) {
        return Err(FetchFailure::permanent("response_too_large"));
    }
    let mut stream = response.bytes_stream();
    let mut body = Vec::with_capacity(declared.unwrap_or(0).min(body_limit as u64) as usize);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| FetchFailure::retryable("body_read_failure"))?;
        if body.len().saturating_add(chunk.len()) > body_limit {
            return Err(FetchFailure::permanent("response_too_large"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

async fn save_page(
    state: &WorkerState,
    claim: &WorkClaim,
    page: PageFetch,
    host_delay_ms: i64,
) -> Result<()> {
    let body_len = page.body.len() as i64;
    let digest = format!("{:x}", Sha256::digest(&page.body));
    let mut tx = state.pool.begin().await?;
    if !claim_is_owned(&mut tx, claim).await? {
        tx.rollback().await?;
        return Ok(());
    }
    if !archive_budget_available(&mut tx, body_len).await? {
        fail_for_archive_quota(&mut tx, claim, page.status, host_delay_ms).await?;
        tx.commit().await?;
        return Ok(());
    }

    insert_page_snapshot(&mut tx, claim, &page, digest).await?;
    sqlx::query("UPDATE archive_budget SET used_body_bytes = used_body_bytes + $1, updated_at = now() WHERE singleton = TRUE")
        .bind(body_len).execute(&mut *tx).await?;
    complete_frontier_url(&mut tx, claim, page.status, host_delay_ms).await?;
    enqueue_discovered(&mut tx, claim, &page.discovered).await?;
    tx.commit().await?;
    Ok(())
}

async fn claim_is_owned(tx: &mut Transaction<'_, Postgres>, claim: &WorkClaim) -> Result<bool> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM frontier_urls WHERE id = $1 AND state = 'leased' AND lease_token = $2)",
    )
    .bind(claim.frontier_id)
    .bind(claim.lease_token)
    .fetch_one(&mut **tx)
    .await
    .map_err(Into::into)
}

async fn archive_budget_available(
    tx: &mut Transaction<'_, Postgres>,
    body_len: i64,
) -> Result<bool> {
    let (used, maximum) = sqlx::query_as::<_, (i64, i64)>(
        "SELECT used_body_bytes, max_body_bytes FROM archive_budget WHERE singleton = TRUE FOR UPDATE",
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(used.saturating_add(body_len) <= maximum)
}

async fn fail_for_archive_quota(
    tx: &mut Transaction<'_, Postgres>,
    claim: &WorkClaim,
    status: u16,
    host_delay_ms: i64,
) -> Result<()> {
    sqlx::query("UPDATE frontier_urls SET state = 'failed', last_error = $3, http_status = $4, completed_at = now(), lease_token = NULL, lease_expires_at = NULL WHERE id = $1 AND lease_token = $2")
        .bind(claim.frontier_id)
        .bind(claim.lease_token)
        .bind(ARCHIVE_QUOTA_ERROR)
        .bind(i32::from(status))
        .execute(&mut **tx)
        .await?;
    release_host(tx, &claim.hostname, claim.lease_token, host_delay_ms).await
}

async fn insert_page_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    claim: &WorkClaim,
    page: &PageFetch,
    digest: String,
) -> Result<()> {
    sqlx::query("INSERT INTO page_snapshots (job_id, frontier_url_id, canonical_url, final_url, hostname, title, http_status, depth, content_type, body, content_sha256) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) ON CONFLICT (job_id, frontier_url_id) DO NOTHING")
        .bind(claim.job_id)
        .bind(claim.frontier_id)
        .bind(&claim.canonical_url)
        .bind(page.final_url.as_str())
        .bind(&claim.hostname)
        .bind(&page.title)
        .bind(i32::from(page.status))
        .bind(claim.depth)
        .bind(&page.content_type)
        .bind(&page.body)
        .bind(digest)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn complete_frontier_url(
    tx: &mut Transaction<'_, Postgres>,
    claim: &WorkClaim,
    status: u16,
    host_delay_ms: i64,
) -> Result<()> {
    sqlx::query("UPDATE frontier_urls SET state = 'completed', http_status = $3, last_error = NULL, completed_at = now(), lease_token = NULL, lease_expires_at = NULL WHERE id = $1 AND lease_token = $2")
        .bind(claim.frontier_id)
        .bind(claim.lease_token)
        .bind(i32::from(status))
        .execute(&mut **tx)
        .await?;
    release_host(tx, &claim.hostname, claim.lease_token, host_delay_ms).await
}

async fn enqueue_discovered(
    tx: &mut Transaction<'_, Postgres>,
    claim: &WorkClaim,
    discovered: &[crawler_domain::NormalizedUrl],
) -> Result<()> {
    if !job_accepts_discovered_urls(tx, claim).await? {
        return Ok(());
    }
    for link in discovered {
        if !enqueue_candidate(tx, claim, link).await? {
            break;
        }
    }
    Ok(())
}

async fn job_accepts_discovered_urls(
    tx: &mut Transaction<'_, Postgres>,
    claim: &WorkClaim,
) -> Result<bool> {
    if claim.depth >= claim.max_depth {
        return Ok(false);
    }
    let count = frontier_count(tx, claim.job_id).await?;
    if count >= i64::from(claim.max_urls) {
        return Ok(false);
    }
    let running_job = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM crawl_jobs WHERE id = $1 AND status = 'running' FOR UPDATE",
    )
    .bind(claim.job_id)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(running_job.is_some())
}

async fn enqueue_candidate(
    tx: &mut Transaction<'_, Postgres>,
    claim: &WorkClaim,
    link: &crawler_domain::NormalizedUrl,
) -> Result<bool> {
    if frontier_count(tx, claim.job_id).await? >= i64::from(claim.max_urls) {
        return Ok(false);
    }
    let allowed = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (SELECT 1 FROM crawl_job_hosts WHERE job_id = $1 AND hostname = $2)",
    )
    .bind(claim.job_id)
    .bind(&link.host)
    .fetch_one(&mut **tx)
    .await?;
    if !allowed {
        return Ok(true);
    }
    let inserted = sqlx::query_scalar::<_, Uuid>("INSERT INTO frontier_urls (job_id, canonical_url, hostname, depth) VALUES ($1, $2, $3, $4) ON CONFLICT (job_id, canonical_url) DO NOTHING RETURNING id")
        .bind(claim.job_id)
        .bind(&link.canonical)
        .bind(&link.host)
        .bind(claim.depth + 1)
        .fetch_optional(&mut **tx)
        .await?;
    if inserted.is_some() {
        sqlx::query(
            "INSERT INTO host_state (hostname) VALUES ($1) ON CONFLICT (hostname) DO NOTHING",
        )
        .bind(&link.host)
        .execute(&mut **tx)
        .await?;
    }
    Ok(true)
}

async fn frontier_count(tx: &mut Transaction<'_, Postgres>, job_id: Uuid) -> Result<i64> {
    sqlx::query_scalar::<_, i64>("SELECT count(*) FROM frontier_urls WHERE job_id = $1")
        .bind(job_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(Into::into)
}

async fn finish_failure(
    state: &WorkerState,
    claim: &WorkClaim,
    failure: FetchFailure,
    _elapsed: u64,
    host_delay_ms: i64,
) -> Result<()> {
    let next_attempts = claim.attempts + 1;
    let retryable = failure.retryable && next_attempts < MAX_ATTEMPTS;
    let retry_seconds = failure
        .retry_after_seconds
        .unwrap_or_else(|| 2u64.saturating_pow(next_attempts.saturating_sub(1) as u32))
        .min(60);
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE frontier_urls SET state = $3, attempts = $4, next_attempt_at = now() + ($5 * interval '1 second'), last_error = $6, completed_at = CASE WHEN $3 = 'failed' THEN now() ELSE NULL END, lease_token = NULL, lease_expires_at = NULL WHERE id = $1 AND state = 'leased' AND lease_token = $2")
        .bind(claim.frontier_id).bind(claim.lease_token)
        .bind(if retryable { "queued" } else { "failed" })
        .bind(next_attempts).bind(retry_seconds as i64).bind(failure.category)
        .execute(&mut *tx).await?;
    release_host(&mut tx, &claim.hostname, claim.lease_token, host_delay_ms).await?;
    tx.commit().await?;
    if retryable {
        warn!(hostname = %claim.hostname, error_category = failure.category, retry_seconds, "crawl request will retry");
    } else {
        warn!(hostname = %claim.hostname, error_category = failure.category, "crawl request failed permanently");
    }
    Ok(())
}

async fn finish_skipped(
    state: &WorkerState,
    claim: &WorkClaim,
    category: &'static str,
    http_status: Option<i32>,
) -> Result<()> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE frontier_urls SET state = 'skipped', last_error = $3, http_status = $4, completed_at = now(), lease_token = NULL, lease_expires_at = NULL WHERE id = $1 AND state = 'leased' AND lease_token = $2")
        .bind(claim.frontier_id).bind(claim.lease_token).bind(category).bind(http_status)
        .execute(&mut *tx).await?;
    release_host(&mut tx, &claim.hostname, claim.lease_token, 0).await?;
    tx.commit().await?;
    Ok(())
}

async fn defer_claim(
    state: &WorkerState,
    claim: &WorkClaim,
    until: DateTime<Utc>,
    throttle: bool,
) -> Result<()> {
    let mut tx = state.pool.begin().await?;
    sqlx::query("UPDATE frontier_urls SET state = 'queued', attempts = greatest(0, attempts - 1), next_attempt_at = greatest(now(), $3), lease_token = NULL, lease_expires_at = NULL WHERE id = $1 AND state = 'leased' AND lease_token = $2")
        .bind(claim.frontier_id).bind(claim.lease_token).bind(until).execute(&mut *tx).await?;
    release_host(
        &mut tx,
        &claim.hostname,
        claim.lease_token,
        if throttle { MIN_HOST_DELAY_MS } else { 0 },
    )
    .await?;
    tx.commit().await?;
    Ok(())
}

async fn release_host(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    hostname: &str,
    token: Uuid,
    delay_ms: i64,
) -> Result<()> {
    sqlx::query("UPDATE host_state SET lease_token = NULL, lease_expires_at = NULL, next_request_at = greatest(next_request_at, now() + ($3 * interval '1 millisecond')) WHERE hostname = $1 AND lease_token = $2")
        .bind(hostname).bind(token).bind(delay_ms).execute(&mut **tx).await?;
    Ok(())
}

async fn set_host_next_request(pool: &PgPool, claim: &WorkClaim, delay_ms: i64) -> Result<()> {
    sqlx::query("UPDATE host_state SET next_request_at = greatest(next_request_at, now() + ($3 * interval '1 millisecond')) WHERE hostname = $1 AND lease_token = $2")
        .bind(&claim.hostname)
        .bind(claim.lease_token)
        .bind(delay_ms.max(MIN_HOST_DELAY_MS))
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crawler_domain::MAX_DEPTH;
    use reqwest::header::{HeaderMap, HeaderValue};
    use sqlx::postgres::PgPoolOptions;

    static DATABASE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    async fn test_pool() -> PgPool {
        let url = std::env::var("TEST_DATABASE_URL")
            .expect("set TEST_DATABASE_URL to an isolated disposable PostgreSQL database");
        let pool = PgPoolOptions::new()
            .max_connections(6)
            .connect(&url)
            .await
            .expect("connect to disposable PostgreSQL");
        MIGRATOR
            .run(&pool)
            .await
            .expect("apply PostgreSQL migrations");
        pool
    }

    async fn queued_claim(pool: &PgPool, hostname: &str, canonical_url: &str) -> WorkClaim {
        let job_id = Uuid::new_v4();
        sqlx::query("INSERT INTO crawl_jobs (id, idempotency_key, request_fingerprint, status, max_urls, max_depth) VALUES ($1, $2, $3, 'running', 10, 3)")
            .bind(job_id)
            .bind(format!("worker-test-{}", Uuid::new_v4()))
            .bind("f".repeat(64))
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO crawl_job_hosts (job_id, hostname) VALUES ($1, $2)")
            .bind(job_id)
            .bind(hostname)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO host_state (hostname) VALUES ($1) ON CONFLICT (hostname) DO UPDATE SET lease_token = NULL, lease_expires_at = NULL, next_request_at = now()")
            .bind(hostname)
            .execute(pool)
            .await
            .unwrap();
        let frontier_id = sqlx::query_scalar::<_, Uuid>("INSERT INTO frontier_urls (job_id, canonical_url, hostname, depth) VALUES ($1, $2, $3, 0) RETURNING id")
            .bind(job_id)
            .bind(canonical_url)
            .bind(hostname)
            .fetch_one(pool)
            .await
            .unwrap();
        WorkClaim {
            frontier_id,
            job_id,
            canonical_url: canonical_url.to_owned(),
            hostname: hostname.to_owned(),
            depth: 0,
            max_depth: 3,
            max_urls: 10,
            attempts: 0,
            lease_token: Uuid::new_v4(),
        }
    }

    async fn lease(pool: &PgPool, claim: &WorkClaim) {
        sqlx::query("UPDATE frontier_urls SET state = 'leased', attempts = 1, lease_token = $2, lease_expires_at = now() + interval '45 seconds' WHERE id = $1")
            .bind(claim.frontier_id)
            .bind(claim.lease_token)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("UPDATE host_state SET lease_token = $2, lease_expires_at = now() + interval '45 seconds' WHERE hostname = $1")
            .bind(&claim.hostname)
            .bind(claim.lease_token)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn cleanup(pool: &PgPool, claim: &WorkClaim) {
        let bytes = sqlx::query_scalar::<_, i64>("SELECT coalesce(sum(octet_length(body)), 0)::bigint FROM page_snapshots WHERE job_id = $1")
            .bind(claim.job_id)
            .fetch_one(pool)
            .await
            .unwrap();
        sqlx::query("UPDATE archive_budget SET used_body_bytes = greatest(0, used_body_bytes - $1) WHERE singleton = TRUE")
            .bind(bytes)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM crawl_jobs WHERE id = $1")
            .bind(claim.job_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM host_state WHERE hostname = $1 AND lease_token IS NULL")
            .bind(&claim.hostname)
            .execute(pool)
            .await
            .unwrap();
    }

    fn worker_state(pool: PgPool) -> WorkerState {
        WorkerState {
            pool,
            user_agent: "WebCrawler/0.1".to_owned(),
            product_token: "WebCrawler".to_owned(),
        }
    }

    fn claim(hostname: &str) -> WorkClaim {
        WorkClaim {
            frontier_id: Uuid::nil(),
            job_id: Uuid::nil(),
            canonical_url: format!("https://{hostname}/"),
            hostname: hostname.to_owned(),
            depth: 0,
            max_depth: MAX_DEPTH,
            max_urls: 500,
            attempts: 0,
            lease_token: Uuid::nil(),
        }
    }

    #[test]
    fn response_limits_and_default_policy_are_bounded() {
        assert_eq!(MAX_RESPONSE_BYTES, 2 * 1024 * 1024);
        assert_eq!(MAX_ATTEMPTS, 3);
        assert_eq!(MAX_DEPTH, 3);
        assert_eq!(MAX_REDIRECTS, 5);
    }

    #[test]
    fn retry_backoff_is_exponential_and_capped() {
        assert_eq!(2u64.saturating_pow(0).min(60), 1);
        assert_eq!(2u64.saturating_pow(1).min(60), 2);
        assert_eq!(2u64.saturating_pow(8).min(60), 60);
    }

    #[test]
    fn retryable_http_failure_caps_retry_after_and_ignores_client_errors() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::RETRY_AFTER,
            HeaderValue::from_static("120"),
        );
        let response = HttpResponse {
            status: 429,
            headers,
            body: Vec::new(),
        };

        let failure = retryable_http_failure(&response).expect("429 is retryable");
        assert_eq!(failure.category, "http_429");
        assert_eq!(failure.retry_after_seconds, Some(60));
        assert!(failure.retryable);

        let client_error = HttpResponse {
            status: 404,
            headers: HeaderMap::new(),
            body: Vec::new(),
        };
        assert!(retryable_http_failure(&client_error).is_none());

        let server_error = HttpResponse {
            status: 503,
            headers: HeaderMap::new(),
            body: Vec::new(),
        };
        assert_eq!(
            retryable_http_failure(&server_error).unwrap().category,
            "http_5xx"
        );
    }

    #[test]
    fn redirects_are_canonicalized_and_remain_on_the_allowed_host() {
        let current = Url::parse("https://example.org/start").unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::LOCATION,
            HeaderValue::from_static("/next#fragment"),
        );
        let response = HttpResponse {
            status: 302,
            headers,
            body: Vec::new(),
        };
        let claim = claim("example.org");
        let allowed_hosts = HashSet::from(["example.org".to_owned()]);

        let target = redirect_target(&current, &response, &claim, &allowed_hosts, 0).unwrap();
        assert_eq!(target.as_str(), "https://example.org/next");
        assert!(
            redirect_target(&current, &response, &claim, &allowed_hosts, MAX_REDIRECTS).is_err()
        );
    }

    #[test]
    fn extracted_links_are_deduplicated_and_stay_within_allowlist_and_depth() {
        let document = Html::parse_document(
            "<a href='/next#first'>one</a><a href='/next#second'>two</a><a href='https://outside.example/'>outside</a>",
        );
        let current = Url::parse("https://example.org/start").unwrap();
        let claim = claim("example.org");
        let allowed_hosts = HashSet::from(["example.org".to_owned()]);

        let links = discover_links(&document, &current, &claim, &allowed_hosts).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].canonical, "https://example.org/next");

        let mut deepest = claim;
        deepest.depth = deepest.max_depth;
        assert!(
            discover_links(&document, &current, &deepest, &allowed_hosts)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn response_parsing_normalizes_title_and_ignores_unusable_links() {
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            HeaderValue::from_static("Text/HTML; charset=utf-8"),
        );
        let claim = claim("example.org");
        let response = HttpResponse {
            status: 200,
            headers,
            body:
                b"<title>  A\n  title </title><a href='javascript:alert(1)'>bad</a><a>missing</a>"
                    .to_vec(),
        };
        let page = page_from_response(
            response,
            Url::parse("https://example.org/").unwrap(),
            &claim,
            &HashSet::from(["example.org".to_owned()]),
        )
        .unwrap();
        assert_eq!(page.title, "A title");
        assert_eq!(page.content_type, "text/html");
        assert!(page.discovered.is_empty());

        let non_html = HttpResponse {
            status: 200,
            headers: HeaderMap::new(),
            body: Vec::new(),
        };
        assert_eq!(
            page_from_response(
                non_html,
                Url::parse("https://example.org/").unwrap(),
                &claim,
                &HashSet::from(["example.org".to_owned()]),
            )
            .unwrap_err()
            .category,
            "non_html_content"
        );
    }

    #[test]
    fn redirect_rejects_missing_location_and_cross_host_targets() {
        let current = Url::parse("https://example.org/start").unwrap();
        let claim = claim("example.org");
        let allowed = HashSet::from(["example.org".to_owned()]);
        let no_location = HttpResponse {
            status: 302,
            headers: HeaderMap::new(),
            body: Vec::new(),
        };
        assert_eq!(
            redirect_target(&current, &no_location, &claim, &allowed, 0)
                .unwrap_err()
                .category,
            "invalid_redirect"
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            reqwest::header::LOCATION,
            HeaderValue::from_static("https://outside.example/"),
        );
        let outside = HttpResponse {
            status: 302,
            headers,
            body: Vec::new(),
        };
        assert_eq!(
            redirect_target(&current, &outside, &claim, &allowed, 0)
                .unwrap_err()
                .category,
            "redirect_host_rejected"
        );
    }

    #[tokio::test]
    async fn fetch_url_validation_enforces_origin_and_robots_path() {
        let pool = test_pool().await;
        let state = worker_state(pool.clone());
        let claim = claim("example.org");
        let initial = Url::parse("https://example.org/start").unwrap();
        let allowed = HashSet::from(["example.org".to_owned()]);
        validate_fetch_url(
            &initial,
            &initial,
            &claim,
            "User-agent: *\nAllow: /",
            &state,
            &allowed,
        )
        .unwrap();
        let cross_origin = Url::parse("http://example.org/start").unwrap();
        assert_eq!(
            validate_fetch_url(&cross_origin, &initial, &claim, "", &state, &allowed)
                .unwrap_err()
                .category,
            "redirect_robots_rejected"
        );
        let private_path = Url::parse("https://example.org/private").unwrap();
        assert_eq!(
            validate_fetch_url(
                &private_path,
                &initial,
                &claim,
                "User-agent: WebCrawler\nDisallow: /private",
                &state,
                &allowed,
            )
            .unwrap_err()
            .category,
            "redirect_robots_rejected"
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn request_once_fails_closed_for_private_and_unresolvable_destinations() {
        let private = request_once("http://127.0.0.1/", "WebCrawler/0.1", 128)
            .await
            .unwrap_err();
        assert_eq!(private.category, "non_public_destination");
        let malformed = request_once("not a URL", "WebCrawler/0.1", 128)
            .await
            .unwrap_err();
        assert_eq!(malformed.category, "invalid_url");
        let dns = request_once("https://crawler.invalid/", "WebCrawler/0.1", 128)
            .await
            .unwrap_err();
        assert!(matches!(
            dns.category,
            "dns_failure" | "non_public_destination"
        ));
    }

    #[tokio::test]
    async fn worker_claim_and_persistence_paths_update_frontier_and_archive() {
        let _guard = DATABASE_TEST_LOCK.lock().await;
        let pool = test_pool().await;
        let hostname = format!("claim-{}.example.test", Uuid::new_v4().simple());
        let url = format!("https://{hostname}/");
        let claim = queued_claim(&pool, &hostname, &url).await;
        let first = claim_next(&pool)
            .await
            .unwrap()
            .expect("due URL is claimable");
        assert_eq!(first.frontier_id, claim.frontier_id);
        assert!(
            claim_next(&pool).await.unwrap().is_none(),
            "host lease prevents concurrent claim"
        );

        let page_url = Url::parse(&url).unwrap();
        let page = PageFetch {
            status: 200,
            final_url: page_url,
            content_type: "text/html".to_owned(),
            body: b"<title>Saved</title>".to_vec(),
            title: "Saved".to_owned(),
            discovered: vec![
                normalize_url(&format!("https://{hostname}/child")).unwrap(),
                normalize_url("https://outside.example/child").unwrap(),
            ],
        };
        save_page(&worker_state(pool.clone()), &first, page, 1_000)
            .await
            .unwrap();
        let state: String = sqlx::query_scalar("SELECT state FROM frontier_urls WHERE id = $1")
            .bind(claim.frontier_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(state, "completed");
        let pages: i64 =
            sqlx::query_scalar("SELECT count(*) FROM page_snapshots WHERE job_id = $1")
                .bind(claim.job_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(pages, 1);
        let links: Vec<String> = sqlx::query_scalar(
            "SELECT canonical_url FROM frontier_urls WHERE job_id = $1 AND id <> $2",
        )
        .bind(claim.job_id)
        .bind(claim.frontier_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(links, vec![format!("https://{hostname}/child")]);
        cleanup(&pool, &claim).await;
        pool.close().await;
    }

    #[tokio::test]
    async fn worker_retries_then_fails_and_marks_skipped_or_deferred_work() {
        let _guard = DATABASE_TEST_LOCK.lock().await;
        let pool = test_pool().await;
        let hostname = format!("retry-{}.example.test", Uuid::new_v4().simple());
        let retry_claim =
            queued_claim(&pool, &hostname, &format!("https://{hostname}/retry")).await;
        lease(&pool, &retry_claim).await;
        finish_failure(
            &worker_state(pool.clone()),
            &retry_claim,
            FetchFailure::retryable("connect_failure"),
            0,
            MIN_HOST_DELAY_MS,
        )
        .await
        .unwrap();
        let row: (String, i32, Option<String>) =
            sqlx::query_as("SELECT state, attempts, last_error FROM frontier_urls WHERE id = $1")
                .bind(retry_claim.frontier_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(
            row,
            ("queued".to_owned(), 1, Some("connect_failure".to_owned()))
        );
        lease(&pool, &retry_claim).await;
        let mut final_attempt = retry_claim.clone();
        final_attempt.attempts = 2;
        finish_failure(
            &worker_state(pool.clone()),
            &final_attempt,
            FetchFailure::retryable("connect_failure"),
            0,
            MIN_HOST_DELAY_MS,
        )
        .await
        .unwrap();
        let failed: String = sqlx::query_scalar("SELECT state FROM frontier_urls WHERE id = $1")
            .bind(retry_claim.frontier_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(failed, "failed");
        cleanup(&pool, &retry_claim).await;

        let skip_host = format!("skip-{}.example.test", Uuid::new_v4().simple());
        let skip_claim =
            queued_claim(&pool, &skip_host, &format!("https://{skip_host}/skip")).await;
        lease(&pool, &skip_claim).await;
        finish_skipped(
            &worker_state(pool.clone()),
            &skip_claim,
            "robots_disallowed",
            Some(403),
        )
        .await
        .unwrap();
        let skipped: (String, Option<i32>) =
            sqlx::query_as("SELECT state, http_status FROM frontier_urls WHERE id = $1")
                .bind(skip_claim.frontier_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(skipped, ("skipped".to_owned(), Some(403)));
        cleanup(&pool, &skip_claim).await;

        let defer_host = format!("defer-{}.example.test", Uuid::new_v4().simple());
        let defer_claim =
            queued_claim(&pool, &defer_host, &format!("https://{defer_host}/defer")).await;
        lease(&pool, &defer_claim).await;
        defer_claim_fn(&pool, &defer_claim).await;
        cleanup(&pool, &defer_claim).await;
        pool.close().await;
    }

    async fn defer_claim_fn(pool: &PgPool, claim: &WorkClaim) {
        defer_claim(
            &worker_state(pool.clone()),
            claim,
            Utc::now() + chrono::Duration::seconds(1),
            true,
        )
        .await
        .unwrap();
        let row: (String, i32) =
            sqlx::query_as("SELECT state, attempts FROM frontier_urls WHERE id = $1")
                .bind(claim.frontier_id)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(row, ("queued".to_owned(), 0));
    }

    #[tokio::test]
    async fn worker_uses_cached_robots_outcomes_and_reaps_expired_leases() {
        let _guard = DATABASE_TEST_LOCK.lock().await;
        let pool = test_pool().await;
        let hostname = format!("robots-{}.example.test", Uuid::new_v4().simple());
        let url = format!("https://{hostname}/private");
        let claim = queued_claim(&pool, &hostname, &url).await;
        lease(&pool, &claim).await;
        let origin = format!("https://{hostname}");
        sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'rules', 'User-agent: WebCrawler' || chr(10) || 'Disallow: /private', now() + interval '1 hour')")
            .bind(&origin)
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            ensure_robots(&worker_state(pool.clone()), &claim, &url)
                .await
                .unwrap(),
            RobotsDecision::Disallowed
        ));
        process_claim(&worker_state(pool.clone()), claim.clone())
            .await
            .unwrap();
        let skipped: String = sqlx::query_scalar("SELECT state FROM frontier_urls WHERE id = $1")
            .bind(claim.frontier_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(skipped, "skipped");
        cleanup(&pool, &claim).await;

        let allowed_host = format!("robots-allow-{}.example.test", Uuid::new_v4().simple());
        let allowed_url = format!("https://{allowed_host}/page");
        let allowed_claim = queued_claim(&pool, &allowed_host, &allowed_url).await;
        lease(&pool, &allowed_claim).await;
        sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'allow', '', now() + interval '1 hour')")
            .bind(format!("https://{allowed_host}"))
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            ensure_robots(&worker_state(pool.clone()), &allowed_claim, &allowed_url)
                .await
                .unwrap(),
            RobotsDecision::Allowed {
                delay_ms: MIN_HOST_DELAY_MS,
                ..
            }
        ));
        cleanup(&pool, &allowed_claim).await;

        let rules_host = format!("robots-rules-{}.example.test", Uuid::new_v4().simple());
        let rules_url = format!("https://{rules_host}/public");
        let rules_claim = queued_claim(&pool, &rules_host, &rules_url).await;
        lease(&pool, &rules_claim).await;
        sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'rules', 'User-agent: WebCrawler' || chr(10) || 'Crawl-delay: 2', now() + interval '1 hour')")
            .bind(format!("https://{rules_host}"))
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            ensure_robots(&worker_state(pool.clone()), &rules_claim, &rules_url)
                .await
                .unwrap(),
            RobotsDecision::Allowed {
                delay_ms: 2_000,
                ..
            }
        ));
        cleanup(&pool, &rules_claim).await;

        let defer_host = format!("robots-defer-{}.example.test", Uuid::new_v4().simple());
        let defer_url = format!("https://{defer_host}/");
        let defer_robots_claim = queued_claim(&pool, &defer_host, &defer_url).await;
        lease(&pool, &defer_robots_claim).await;
        sqlx::query("INSERT INTO robots_cache (origin, outcome, rules, expires_at) VALUES ($1, 'defer', '', now() + interval '1 minute')")
            .bind(format!("https://{defer_host}"))
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            ensure_robots(&worker_state(pool.clone()), &defer_robots_claim, &defer_url)
                .await
                .unwrap(),
            RobotsDecision::Deferred(_)
        ));
        cleanup(&pool, &defer_robots_claim).await;

        let invalid_host = format!("invalid-{}.example.test", Uuid::new_v4().simple());
        let invalid_claim = queued_claim(&pool, &invalid_host, "not-a-url").await;
        lease(&pool, &invalid_claim).await;
        process_claim(&worker_state(pool.clone()), invalid_claim.clone())
            .await
            .unwrap();
        let invalid_state: String =
            sqlx::query_scalar("SELECT state FROM frontier_urls WHERE id = $1")
                .bind(invalid_claim.frontier_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(invalid_state, "failed");
        cleanup(&pool, &invalid_claim).await;

        let expired_host = format!("expired-{}.example.test", Uuid::new_v4().simple());
        let expired_url = format!("https://{expired_host}/");
        let expired_claim = queued_claim(&pool, &expired_host, &expired_url).await;
        lease(&pool, &expired_claim).await;
        sqlx::query(
            "UPDATE frontier_urls SET lease_expires_at = now() - interval '1 minute' WHERE id = $1",
        )
        .bind(expired_claim.frontier_id)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("UPDATE host_state SET lease_expires_at = now() - interval '1 minute' WHERE hostname = $1")
            .bind(&expired_host)
            .execute(&pool)
            .await
            .unwrap();
        reap_and_complete(&pool).await.unwrap();
        let state: String = sqlx::query_scalar("SELECT state FROM frontier_urls WHERE id = $1")
            .bind(expired_claim.frontier_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(state, "queued");
        cleanup(&pool, &expired_claim).await;
        pool.close().await;
    }
}
