CREATE TABLE crawl_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    idempotency_key TEXT NOT NULL UNIQUE,
    request_fingerprint CHAR(64) NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('running', 'completed', 'completed_with_errors')),
    max_urls INTEGER NOT NULL CHECK (max_urls BETWEEN 1 AND 500),
    max_depth INTEGER NOT NULL CHECK (max_depth BETWEEN 0 AND 3),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ
);

CREATE TABLE crawl_job_hosts (
    job_id UUID NOT NULL REFERENCES crawl_jobs(id) ON DELETE CASCADE,
    hostname TEXT NOT NULL,
    PRIMARY KEY (job_id, hostname)
);

-- The shared host state is deliberately global across jobs and worker pods.
CREATE TABLE host_state (
    hostname TEXT PRIMARY KEY,
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    next_request_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE frontier_urls (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES crawl_jobs(id) ON DELETE CASCADE,
    canonical_url TEXT NOT NULL CHECK (octet_length(canonical_url) <= 4096),
    hostname TEXT NOT NULL,
    depth INTEGER NOT NULL CHECK (depth BETWEEN 0 AND 3),
    state TEXT NOT NULL DEFAULT 'queued'
        CHECK (state IN ('queued', 'leased', 'completed', 'skipped', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0 CHECK (attempts BETWEEN 0 AND 3),
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    lease_token UUID,
    lease_expires_at TIMESTAMPTZ,
    last_error TEXT,
    http_status INTEGER,
    discovered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at TIMESTAMPTZ,
    UNIQUE (job_id, canonical_url)
);

CREATE INDEX frontier_due_idx ON frontier_urls (next_attempt_at, discovered_at)
    WHERE state = 'queued';
CREATE INDEX frontier_job_state_idx ON frontier_urls (job_id, state);

CREATE TABLE robots_cache (
    origin TEXT PRIMARY KEY,
    outcome TEXT NOT NULL CHECK (outcome IN ('rules', 'allow', 'deny', 'defer')),
    rules TEXT NOT NULL DEFAULT '',
    cached_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX robots_cache_expiry_idx ON robots_cache (expires_at);

CREATE TABLE page_snapshots (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES crawl_jobs(id) ON DELETE CASCADE,
    frontier_url_id UUID NOT NULL REFERENCES frontier_urls(id) ON DELETE CASCADE,
    canonical_url TEXT NOT NULL,
    final_url TEXT NOT NULL,
    hostname TEXT NOT NULL,
    title TEXT NOT NULL,
    http_status INTEGER NOT NULL,
    depth INTEGER NOT NULL,
    content_type TEXT NOT NULL,
    body BYTEA NOT NULL,
    content_sha256 CHAR(64) NOT NULL,
    fetched_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (job_id, frontier_url_id)
);

CREATE INDEX page_job_fetched_idx ON page_snapshots (job_id, fetched_at DESC, id DESC);

CREATE TABLE archive_budget (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    max_body_bytes BIGINT NOT NULL CHECK (max_body_bytes > 0),
    used_body_bytes BIGINT NOT NULL DEFAULT 0 CHECK (used_body_bytes >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
INSERT INTO archive_budget (singleton, max_body_bytes, used_body_bytes)
VALUES (TRUE, 6442450944, 0)
ON CONFLICT (singleton) DO NOTHING;
