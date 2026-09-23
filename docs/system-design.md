# Web Crawler system design

- **Document status:** implemented v1, local-development scope
- **Updated:** 2026-09-24
- **Application revision:** the Rust and Vue service scopes are analyzed from committed source; the verification record reports the date and branch for the latest SonarQube run.
- **Canonical design:** this Markdown file is the sole system-design document.

> Project documentation index: [Documentation index](README.md)

> Project decision records: [ADR index](adr/README.md).

> Database tables, columns, and ownership: [Database model](database-model.md).

## Contents

- [1. Abstract](#1-abstract)
- [2. Goals and non-goals](#2-goals-and-non-goals)
- [3. Background and problem statement](#3-background-and-problem-statement)
- [4. Proposed architecture and ownership](#4-proposed-architecture-and-ownership)
- [5. Request lifecycle, crawl behavior, and limits](#5-request-lifecycle-crawl-behavior-and-limits)
- [Security and privacy considerations](#security-and-privacy-considerations)
- [6. API and data contracts](#6-api-and-data-contracts)
- [Consistency, idempotency, and replay](#consistency-idempotency-and-replay)
- [7. Data model and database recommendation](#7-data-model-and-database-recommendation)
- [Alternatives considered](#alternatives-considered)
- [8. Operational readiness and deployment](#8-operational-readiness-and-deployment)
- [9. Verification and acceptance](#9-verification-and-acceptance)
- [Open questions and next decisions](#open-questions-and-next-decisions)
- [Decision and next steps](#decision-and-next-steps)
- [10. References and traceability](#10-references-and-traceability)
- [Glossary](#glossary)

## 1. Abstract

Web Crawler is a private, on-demand HTML archive. A user submits seed URLs and an exact hostname allowlist, follows the job, reviews bounded saved page source as inert text, and deletes the job and its content.

The application has three separately built and deployed services:

- **Frontend:** Vue 3 and TypeScript, served by NGINX.
- **API:** Rust/Axum, responsible for job creation, input validation, status, page reads, and deletion.
- **Worker:** Rust/Tokio, responsible for durable frontier claims, robots policy, safe outbound fetches, HTML extraction, and page persistence.

Each application service lives in its own named repository subfolder. Each has its own README.md, AGENTS.md, Dockerfile, Deployment, and pods. The Helm release configures at least two replicas for each application Deployment. They are separate pods; the API and worker are not sidecars in one pod. PostgreSQL is a shared, single-replica local StatefulSet, not an application microservice and not highly available.

**PostgreSQL is the recommended and implemented v1 database.** It stores job state, the URL frontier, global host leases, robots cache, page metadata and bounded response bodies. Transactions, uniqueness constraints, and row locking provide the coordination the local scale needs without Redis, a broker, or a separate queue.

This is a local tool, not a public crawler product. The cluster exposes only internal ClusterIP Services. Users reach the UI through a loopback-bound port-forward. The UI has no account system or authentication, so do not expose the application outside a trusted local environment.

### Evidence and status

The first implementation baseline is the documentation-only commit above. Source, tests, migrations, chart, and service guidance have been added in the worktree since that commit. Use code and generated artifacts for implemented behavior; use this document for system-level decisions and limits.

| Evidence kind | Current finding |
| --- | --- |
| Implemented source | Rust workspace in Cargo.toml; shared policy in crates/crawler-domain/; API in crawler-api/; worker in crawler-worker/; Vue UI in crawler-frontend/. |
| Persistence | Versioned initial schema at database/postgres/migrations/0001_initial.sql; SQLx API startup applies embedded migrations. |
| HTTP contract | docs/api/openapi.yaml. |
| Local deployment | deploy/helm/web-crawler/; separate API, worker, and frontend Deployments; PostgreSQL StatefulSet; internal Services. |
| Tests | Rust unit and PostgreSQL contract tests, Vue unit tests, and Playwright browser flow are present. This document distinguishes each test level and records the commands run in its verification table below. |
| Tool availability | OpenDesign and Jev were not available as callable tools in this implementation session. The frontend design notes record the OpenDesign limitation; no Jev score or review is claimed. SonarQube is available as a local server, but a passing current-revision quality gate is only reported if scanner analysis and the result are verified. |
| Environment observation | On 2026-09-24, current context was kind-kind, cluster kind, with one kind-control-plane v1.37.0 node and default standard local-path StorageClass. Helm reported v4.3.0. This is a dated machine observation, not a repository guarantee. |

Statements below are tagged by context: **Implemented** describes current code paths; **Decision** is the v1 product/architecture choice; **Limitation** names behavior that is partial or intentionally absent. An uncommitted worktree is not a release artifact. Recheck exact revisions and commands before deployment.

## 2. Goals and non-goals

Requirement IDs are stable references for tests and review.

| ID | Requirement | v1 behavior / acceptance |
| --- | --- | --- |
| WC-01 | Create bounded jobs | Accept 1–20 HTTP(S) seeds and 1–20 exact host entries; canonicalize and deduplicate them; every seed host must be explicitly allowed. |
| WC-02 | Enforce crawl boundaries | The worker checks robots.txt, uses global hostname leases, blocks unsafe resolved addresses, disables proxy and automatic redirect behavior, and caps each redirect chain. |
| WC-03 | Persist progress | Jobs, frontier, host coordination, robots outcomes, and page snapshots survive application pod replacement while the PostgreSQL PVC survives. |
| WC-04 | Bound resource use | Cap each job at 500 unique URLs and depth 3; each response at 2 MiB decompressed; each page attempt at 10 seconds; cap retries, links, redirects, and global retained body bytes. |
| WC-05 | Review safely | Return snapshots as plain text with nosniff; the Vue app displays source through escaped interpolation. |
| WC-06 | Delete retained data | User deletion removes the job and its job-owned frontier, snapshots, and content, and returns body bytes to the global quota. |
| WC-07 | Run locally | Deploy the three app services separately with at least two replicas each, plus one PostgreSQL StatefulSet, through Helm on Kind. Keep every Service internal. |
| WC-08 | Maintain distinct service ownership | Keep frontend, API, and worker in crawler-frontend/, crawler-api/, and crawler-worker/, each with an owned README and AGENTS guide. |

Non-goals: authentication/accounts, public exposure, scheduled recrawls, JavaScript rendering, non-HTML archiving, full-text search, cross-region crawling, production availability/backups, and search-engine-scale throughput. ByteByteGo’s billion-pages-per-month example is a scale comparison only; v1 is at most hundreds of URLs per user-created job.

## 3. Background and problem statement

The v1 system accepts explicit on-demand crawl jobs, restricts fetches to exact user-supplied hostnames, applies hard resource and safety bounds, and stores fetched HTML for later inspection. It is intended for loopback use in a local Kind cluster; authentication, scheduled recrawls, JavaScript rendering, public exposure, and a backup/restore workflow are outside the current scope.

## 4. Proposed architecture and ownership

### Domain language

- **Crawl Job:** user request plus normalized seeds, exact allowed hosts, hard-bounded limits, lifecycle, and result counts.
- **Seed URL:** a user-supplied starting URL.
- **Allowed Host:** one normalized DNS hostname authorized for this job. Authorization does not imply subdomains.
- **Frontier URL:** a canonical URL owned by one job and waiting, leased, completed, skipped, or failed.
- **Host Lease:** a durable, hostname-wide claim shared across jobs and worker replicas.
- **Robots Policy:** cached origin rules, allow/deny outcome, and expiry.
- **Page Snapshot:** metadata and original response bytes saved for one fetched frontier URL.

### Deployment and code ownership

| Service | Folder | Responsibilities | Helm workload | Default replicas |
| --- | --- | --- | --- | ---: |
| Frontend, web-crawler-frontend | crawler-frontend/ | Vue UI, accessibility, API calls, NGINX same-origin /api proxy | Deployment + internal Service | 2 |
| API, web-crawler-api | crawler-api/ | Request validation, idempotent job admission, status/page reads, content delivery, deletion | Deployment + internal Service | 2 |
| Worker, web-crawler-worker | crawler-worker/ | Frontier claim/recovery, host coordination, robots, outbound fetch, parsing, bounded persistence | Deployment + health listener | 2 |
| PostgreSQL | database/postgres/ | Durable shared authority and archive | StatefulSet + internal Service + PVC | 1 |

Each application service has its own folder-level README.md and AGENTS.md; keep service contracts and run commands there as focused pointers to the root documents. Each service has its own image and pods. PostgreSQL is not a fourth application microservice.

The design uses DDD to name invariants and ownership, not to equate folders with bounded contexts. Crawl admission and crawl execution are distinct deployment roles in a shared Crawl domain. The shared crawler-domain crate holds URL/host policy and domain limits. API and worker deliberately share PostgreSQL and migrations for v1, so they do not have independent data stores or fully autonomous bounded contexts. The API owns user-facing lifecycle operations; the worker owns network execution and page outcomes. Keep this coupling explicit.

~~~mermaid
flowchart LR
    User[Local operator browser] -->|loopback port-forward| FrontSvc[Frontend ClusterIP]
    FrontSvc --> FrontPods[Vue and NGINX Deployment<br/>2 or more pods]
    FrontPods -->|same-origin /api| ApiSvc[API ClusterIP]
    ApiSvc --> ApiPods[Rust API Deployment<br/>2 or more pods]
    ApiPods -->|transactions and reads| DB[(PostgreSQL StatefulSet<br/>one pod and PVC)]
    WorkerPods[Rust worker Deployment<br/>2 or more pods] -->|claim, lease, persist| DB
    WorkerPods --> Policy[Allowlist, robots, host lease]
    Policy --> SafeFetch[DNS validation and pinned HTTP]
    SafeFetch -->|approved public destinations only| Web[Explicitly allowed hosts]
    SafeFetch --> Parse[Bounded HTML parse]
    Parse --> DB
~~~

### Failure-aware crawl flow

~~~mermaid
sequenceDiagram
    actor User
    participant UI as Vue frontend
    participant API as Rust API
    participant DB as PostgreSQL
    participant Worker as Rust worker
    participant Site as Allowed host
    User->>UI: Enter seed URLs and exact host allowlist
    UI->>API: POST job plus Idempotency-Key
    API->>API: Normalize URL, host, and limits
    API->>DB: One transaction: job, hosts, unique seeds
    API-->>UI: 202 Accepted with job ID
    Worker->>DB: Claim due URL and global host lease
    Worker->>Worker: Check robots and validate/pin DNS answers
    alt Robots policy is unavailable
        Worker->>DB: Defer URL and preserve job progress
    else Robots disallows URL
        Worker->>DB: Mark URL skipped and release host lease
    else Destination is unsafe or policy rejects redirect
        Worker->>DB: Mark URL failed and release host lease
    else Target returns transient network error, 429, or 5xx
        Worker->>DB: Retry with bounded backoff or mark failed
    else Bounded HTML/XHTML response
        Worker->>Site: GET under time/body caps
        Worker->>DB: Commit snapshot, discoveries, counters, lease release
    end
    UI->>API: Poll status and keyset page list
    API->>DB: Read current job and snapshots
    API-->>UI: Status, metadata, and plain-text source
    User->>UI: Delete job
    UI->>API: DELETE job
    API->>DB: Lock job, refund body quota, and cascade deletion
~~~

## 5. Request lifecycle, crawl behavior, and limits

These are implemented defaults and hard v1 maxima unless the source path shows otherwise. Per-job max_urls and max_depth may be lowered. No public configuration should raise hard bounds without an architecture and security review.

| Limit or policy | Default / cap | Implementation note |
| --- | ---: | --- |
| Unique URLs per job | 500 | Seeds count toward the cap; a DB uniqueness constraint deduplicates canonical URLs. |
| Depth | 3 | Seeds have depth 0. The worker does not admit discovered links from depth 3. |
| Seeds / allowed hosts | 20 each | Enforced by API request validation. |
| URL length | 4 KiB | Shared normalizer rejects overlong input and canonical output. |
| Links examined per page | 1,000 | Worker selects only the first 1,000 a[href] candidates before filtering. |
| Response body | 2 MiB decompressed | Worker streams the response and aborts once decompressed bytes exceed the limit. |
| Request deadline | 10 seconds | Per robots or page request attempt. Page redirect chain shares the outer worker deadline. |
| Redirects | 5 | Automatic redirects are disabled; only same-host and same-origin page redirects are followed, with policy rechecks. |
| Page attempts | 3 total | Retryable page failures use exponential delays, capped at 60 seconds; an integer Retry-After is honored up to 60 seconds. No random jitter is implemented. |
| Per-host spacing | At least 1 second | A single hostname-wide PostgreSQL lease coordinates all jobs and worker pods. Crawl-delay is honored up to 60 seconds. |
| Robots cache | 1 hour for fetched policy | Temporary failures defer for 30 seconds. Robots requests that redirect are deferred and retried; they are not followed. |
| Global saved body quota | 6 GiB | PostgreSQL quota accounting tracks decompressed retained body bytes, not total PVC usage. New jobs are rejected at quota; a page that would exceed it is failed safely. |
| Automatic retention | None | Jobs remain until explicit deletion. |

The 6 GiB body budget leaves nominal headroom on the 10 Gi PVC for indexes, WAL, PostgreSQL metadata, and free-space management; it does not guarantee that the volume can safely reach the quota. Monitor actual PVC free space. The storage class has no expansion support in the observed local cluster, so a full volume can require operator intervention.

### URL, host, and robots policy

1. Accept only HTTP and HTTPS URLs using default ports. Reject user-info, IP literal hosts, controls, unsupported schemes, malformed values, and URLs over 4 KiB. Canonicalization lowercases/IDNA-normalizes host names, removes fragments, resolves path dot segments, and preserves query semantics.
2. Normalize exact allowlist entries as DNS hostnames. Reject wildcards, IPs, host:port forms, and invalid DNS labels. A host match is exact; no subdomain suffix rule exists.
3. Resolve all A and AAAA records before connecting. Reject the request if any answer is loopback, private, link-local, multicast, reserved, or otherwise outside the implemented public-address ranges. Pin the approved DNS answers in the HTTP client to limit rebinding between validation and connect. Disable environment proxies and implicit redirects.
4. Check robots.txt before pages. Cache fetched policy by origin. A missing file (404/410) allows fetching; 401/403 denies; temporary failures, redirects, and bodies over 512 KiB defer; policy text over 10,000 lines is parsed fail-closed. Respect the selected rules and a maximum supported Crawl-delay.
5. Page redirects must remain on the same hostname and origin. Revalidate normalized URL, host allowlist, robots path rules, DNS answers, and public address on each hop. Do not forward user credentials.
6. Parse only text/html and application/xhtml+xml. Save the received/decompressed response bytes; parse a lossy UTF-8 view only for title and link extraction. Do not run JavaScript or load page assets.
7. Serve saved bytes as text/plain; charset=utf-8 with X-Content-Type-Options: nosniff. Render them using Vue interpolation. Treat source, URL, title, and all remote text as hostile.

**Robots limitation:** crawler-worker/src/robots.rs is a small custom parser supporting user-agent groups, Allow/Disallow, wildcard and end-anchor matching, and Crawl-delay. It is not certified as a complete RFC 9309 implementation. Unsupported syntax or future standards changes need tests and review. If the parser sees more than 10,000 lines it fails closed.

**Privacy and logging:** do not log full URL paths, query strings, credentials, cookies, response bodies, or arbitrary headers. Use IDs, normalized hostname, outcome category, byte counts, and elapsed times. API error bodies return stable categories and a request ID, not SQL or remote content. The crawler’s configured User-Agent and product token default to WebCrawler/0.1.

## Security and privacy considerations

The UI is intended to be reached through loopback port-forwarding. The API validates exact host allowlists and resolved addresses, pins approved public addresses, rejects unsafe redirects, applies response/body limits, and serves archived source as plain text. Authentication and production secret management are not implemented. Logs must omit full paths, query strings, response bodies, cookies, credentials, and arbitrary headers; the archive has no automatic retention or backup.

## 6. API and data contracts

The versioned contract is [OpenAPI](api/openapi.yaml). The browser uses same-origin paths through NGINX; no browser CORS policy or direct API exposure is configured.

| Endpoint | Behavior |
| --- | --- |
| POST /api/v1/crawl-jobs | Requires Idempotency-Key; accepts 1–20 seeds, 1–20 exact allowed_hosts, optional limits. Normalizes and sorts input before fingerprinting. Returns 202 after durable creation. Same key and same normalized request returns the existing job; same key with a different request returns 409. |
| GET /api/v1/crawl-jobs/{job_id} | Returns lifecycle, limits, timestamps, and frontier-state counts. |
| GET /api/v1/crawl-jobs/{job_id}/pages?cursor=…&limit=… | Returns keyset-paginated metadata, default 50 / max 100, ordered by (fetched_at DESC, id DESC). |
| GET /api/v1/crawl-jobs/{job_id}/pages/{page_id}/content | Returns archived source bytes as text/plain; charset=utf-8 plus nosniff. |
| DELETE /api/v1/crawl-jobs/{job_id} | Removes the job and dependent rows; atomically decrements quota bytes; returns 204. |
| GET /health/live | Process liveness. |
| GET /health/ready | Readiness requires SELECT 1 from PostgreSQL. API applies embedded SQLx migrations before it begins listening. |

Create-request errors: malformed JSON, absent/invalid idempotency key, or invalid cursor produce 400; idempotency conflict produces 409; URL, host, or limit violations produce 422; body quota exhaustion produces 507; dependency failure produces 503. The frontend maps categories to human-readable errors.


### Contract ownership and consumers

| Interface | Owner | Producer | Known consumers by repository/component | Authority |
| --- | --- | --- | --- | --- |
| Crawl Jobs and archive REST API | `web-crawler` / `crawler-api` | `crawler-api` | Same repository: `crawler-frontend`; other-repository consumers: unknown | [OpenAPI](api/openapi.yaml) and API implementation |
| NGINX same-origin `/api/v1` proxy | `web-crawler` / `crawler-frontend` owns routing | Browser requests through the frontend | Same repository: `crawler-api`; other-repository consumers: unknown | `crawler-frontend/nginx.conf` |
| Durable frontier and archive schema | `web-crawler`; API owns admission/deletion and worker owns execution results | `crawler-api`, `crawler-worker` | Same repository: API and worker; other-repository consumers: unknown (internal database state) | [PostgreSQL migrations](../database/postgres/migrations/) |

See the [contract catalog](contracts/README.md) for producer/consumer details.

## Consistency, idempotency, and replay

- Job admission persists the normalized request, exact host allowlist, seed frontier, and idempotency fingerprint transactionally before returning `202`.
- Repeating the same key with an equivalent normalized request returns the existing job; reusing it with a different request returns `409`.
- PostgreSQL row locks and host leases coordinate worker replicas. Lease expiry allows another worker to resume after a crash; network fetches can repeat, so the system does not promise exactly-once fetching.
- Unique constraints, lease ownership, and transactional page/frontier updates protect durable state from duplicate worker attempts.
- Deleting a job removes its dependent archive rows and releases its quota accounting. The current deployment has no backup/replay-from-backup workflow.

## 7. Data model and database recommendation

### PostgreSQL is the v1 choice

PostgreSQL is the recommended and implemented sole v1 database because the core workload is transactional and relational: create a job with its allowlist and seed frontier atomically; deduplicate URLs per job; claim work across replicas without double-claiming; coordinate one host lease across jobs; save a page and its newly discovered URLs consistently; delete a job and reclaim its quota. A single local PostgreSQL StatefulSet is simple to run on Kind and avoids operating a separate message broker for modest, on-demand jobs.

The worker’s claim query uses row locks with FOR UPDATE SKIP LOCKED; a lease expiry allows recovery after a worker pod exits. The durable PostgreSQL frontier is also the queue for v1. This gives transactional coordination, not exactly-once network fetching: a request may be repeated after a worker dies, while database effects remain guarded by lease ownership and unique constraints.

### Implemented schema

Migration path: database/postgres/migrations/0001_initial.sql.

| Table | Role and key constraints |
| --- | --- |
| crawl_jobs | UUID, idempotency key and request fingerprint, status, immutable max URL/depth limits, timestamps. |
| crawl_job_hosts | Exact per-job hostname allowlist; primary key (job_id, hostname); job deletion cascades. |
| host_state | Global hostname primary key, lease token/expiry, and next request time shared across all jobs. |
| frontier_urls | Canonical URL, host, depth, state, attempt count, schedule, lease token, error/status; unique (job_id, canonical_url); indexed due frontier. |
| robots_cache | Origin-keyed robots outcome/rules and expiry. |
| page_snapshots | Page metadata and body bytes (BYTEA), content SHA-256, unique (job_id, frontier_url_id); deletion cascades with job/frontier. |
| archive_budget | Singleton global quota row with maximum and used body bytes. |

There is no page_contents table, separate object store, or cross-job content deduplication. Bodies are kept in PostgreSQL because the archive and quota are deliberately small. PostgreSQL data is the authority; browser local storage contains only up to five recent job IDs.

## Alternatives considered

| Option | Why it is not the v1 choice | When to consider it |
| --- | --- | --- |
| Cassandra / ScyllaDB | Excellent horizontally distributed write throughput and partitioned access patterns, but less convenient for atomic job admission, host leases, quota accounting, and relational cascade deletion. It would add operational complexity without a scale need. The sibling URL shortener’s Cassandra choice serves a different key-value/throughput workload and is not transferable. | A measured high-volume frontier where partition design and distributed consistency are understood; likely paired with separate coordination design. |
| SQLite | Very small footprint and simple for one-process/local-only execution, but write serialization and multi-pod shared storage are a poor match for three services and multiple worker replicas. | A single-binary desktop version with one worker and no Kubernetes/multi-replica target. |
| Redis | Fast in-memory queue/lease data, but adds another service and durability/coordination mode. A Redis queue would be harder to atomically commit with job and page state. | A measured hot coordination/cache need, while PostgreSQL remains source of truth and recovery semantics are designed. |
| OpenSearch/Elasticsearch | Not needed because v1 has no full-text search; indexing raw HTML would increase cost and create another consistency path. | Explicitly scoped search after archive volume and relevance requirements are known. |
| S3-compatible object storage | More suitable for very large bodies than PostgreSQL BYTEA, but introduces object/database transaction and orphan cleanup problems. | Body storage grows beyond the local quota or needs independent lifecycle, backup, and streaming behavior. Keep PostgreSQL metadata/frontier. |

### Architecture practice fit

DDD and Clean Architecture are useful for URL/host safety, robots policy, crawl limits, and job/page lifecycle rules. The [`crawler-domain` crate](../crates/crawler-domain/) is the small policy core; API and worker remain deployment roles in the same Crawl domain and use PostgreSQL transactions for frontier claims, host leases, quotas, and persistence. Do not turn those roles into independently owned bounded contexts or add ports that only forward one database call. Full CQRS is not justified because coordination requires a current shared state model and the UI's job/page reads are modest. Revisit a query projection only after measured read load and a tolerable staleness contract. YAGNI, KISS, and DRY mean retaining the present PostgreSQL coordination and one authoritative domain vocabulary instead of adding Redis, a broker, or a separate search store speculatively.

A billion-page-per-month system is far outside this target. It would need a distributed frontier, high-volume host sharding, dedicated large-scale content storage, partitioning, and independent crawl scheduling. It is not a reason to introduce those components into this local archive now.

## 8. Operational readiness and deployment

### Helm / Kubernetes layout

Chart: deploy/helm/web-crawler/.

- One distinct Deployment/pod set for each of Vue frontend, Rust API, and Rust worker; defaults are two replicas each. Each has its own image, probes, CPU/memory requests and limits, non-root UID, dropped capabilities, read-only root filesystem where supported, and disabled service-account token mounting.
- Frontend and API each have internal ClusterIP Services. NGINX proxies /api to API Service. Worker has a health listener, not a user-facing Service.
- One PostgreSQL 17 StatefulSet replica with the existing Kubernetes Secret web-crawler-postgres, a standard 10 Gi PVC, internal ClusterIP Service, and health probes.
- API and worker init containers wait for PostgreSQL readiness before the application containers start. The API applies versioned embedded migrations during startup; there is no separate migration Job. A migration failure blocks API readiness.
- PodDisruptionBudgets preserve one available pod for each app Deployment; a single-node Kind cluster cannot deliver node-level high availability.
- No ingress, NodePort, LoadBalancer, authentication, or NetworkPolicy egress policy is installed. Restrict use to the local cluster and port-forward to 127.0.0.1.

Images are built from repository root so Docker build contexts can include the shared Rust crates and migrations. Kind uses local images and IfNotPresent; remote chart installation or image publication is not configured.

### Local environment observed

On 2026-09-24, the observed Kubernetes context was kind-kind, named cluster kind, a single kind-control-plane node running Kubernetes 1.37.0, and the default standard local-path StorageClass with volume expansion disabled. Helm 4.3.0 and Kind 0.33.0 were available. Recheck current context and cluster before every operation. Never use local instructions against a remote context.

The development workflow builds each image, loads it into that existing Kind cluster, creates a namespaced Secret outside Git, installs/upgrades the chart into web-crawler, checks readiness, and uses loopback-only port-forward for the UI. Root README.md has the exact commands. Creating or deleting a cluster is not part of install. Deleting the namespace/PVC permanently deletes this local archive.

### Operational boundaries

- A failed API database connection prevents readiness. Workers do not intentionally start a new claim while PostgreSQL is unavailable; in-flight results depend on lease ownership and are discarded if the claim is stale.
- A worker process crash leaves claims until lease expiry; another worker can reclaim them. A page may be fetched more than once after failure.
- Jobs with only failed/skipped frontier URLs become completed_with_errors; jobs whose frontier drains without those outcomes become completed.
- Temporary robots failures are deferred, do not consume the page-attempt counter, and can keep a job running until the origin responds.
- The 6 GiB quota covers response body bytes only. PVC total usage also includes WAL, indexes, row overhead, and metadata; monitor free space. There is no automated retention or backup.
- PostgreSQL is one local pod and volume. Pod rescheduling within the node is supported by the PVC; loss of the Kind node/cluster or PVC loses the archive.
- No request authentication is implemented; loopback port-forward is the intended access boundary. Do not expose the internal Services.



### Resource budgets and Kubernetes practice

Every pod template has CPU and memory requests and limits for each regular and init container. The concrete local values are maintained in [Kubernetes resource budgets](kubernetes-resources.md); they are local defaults, not measured consumption or production sizing. Measure representative workloads in the target environment, set requests for observed baseline needs and limits for acceptable bursts, then monitor CPU throttling, memory pressure, and OOM events and adjust deliberately.

## 9. Verification and acceptance

### Verification record

Evidence below was gathered on 2026-09-24. The latest SonarQube run analyzes the committed Rust and Vue service sources on `main`; documentation is outside those project scopes. The previously recorded Kind rollout used the same runtime source. A rendered chart, successful unit suite, or UI mock does not prove a live crawl or production safety.

| Check | Command / scope | Result |
| --- | --- | --- |
| Rust format and lint | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed. |
| Rust tests and coverage | `TEST_DATABASE_URL=... cargo llvm-cov --workspace --all-targets --locked --lcov ...` against a disposable PostgreSQL container | Passed: 31 tests (8 API, 1 PostgreSQL contract, 1 config, 5 domain, 16 worker/robots). Sonar coverage: API 95.4%; worker 81.8%, including shared domain/config crates. |
| Frontend | `npm ci`; `npm run typecheck`; `npm run lint`; `npm run test:unit --coverage`; `npm run build`; `npm run test:e2e` | Passed: 6 unit tests, 90.14% local line coverage, and 1 mocked-API Playwright E2E test. Sonar reports 85.4% coverage. |
| Dependency audit | `npm audit --audit-level=low` | Passed: 0 vulnerabilities. |
| Helm and Kubernetes schema | `helm lint`; `helm template`; `kubectl --context kind-kind apply --dry-run=client -f /tmp/web-crawler-rendered.yaml` | Passed; Helm lint reported only the informational recommendation to add a chart icon. |
| Container images | `docker build` for API, worker, and frontend from repository root | All three built. Frontend image was rebuilt after correcting its NGINX PID path and writable temporary-volume group. |
| Kind rollout and smoke | Helm install/upgrade on inspected `kind-kind`; loopback port-forward; UI/API requests; deployment and PVC inspection | Passed on Helm revision 3: API, worker, and frontend each Ready 2/2; PostgreSQL 1/1; 10 Gi PVC Bound. API/worker PostgreSQL wait init containers completed; the new app pods had zero restarts. UI and `/healthz` returned 200; API create/status/pages/delete returned 202/200/200/204, and a deleted job returned 404. The reserved `.invalid` seed reached the robots-resolution defer path; no public page was fetched. The initial rollout exposed an NGINX PID conflict and a database cold-start race; the image and chart were corrected before revision 3. |
| Jev | Jev readiness scoring | Not available as a callable tool in this session; no score claimed. |
| OpenDesign | Frontend rendered-design review | Not available in this Linux implementation environment; see `crawler-frontend/DESIGN.md`. |
| SonarQube | `.sonar/scan.sh`; projects `web-crawler-api`, `web-crawler-worker`, and `web-crawler-frontend` | Passed on `main` (2026-09-24). All three configured Quality Gates passed (new coverage 95.4%, 81.8%, and 85.4%); each project has 0 open issues, 0 bugs, 0 vulnerabilities, 0 security hotspots, 0 code smells, and 0 duplicated lines (0.0% density). The gate retains its 80% coverage, 3% duplication, and zero-new-violations conditions. |

### Future acceptance scenarios

Keep these scenarios as named follow-up coverage even if a specific current suite does not exercise them:

1. **Normalization and deduplication:** IDNA/case/default-port/fragment handling, query preservation, duplicate seeds/discoveries, invalid schemes, credentials, ports, URL length, wildcard hosts.
2. **Allowlist and robots:** exact-host matching, disallowed path, Allow tie-break, wildcard/end anchor, missing robots, 401/403, temporary outage, redirect, oversized/malformed policy.
3. **SSRF and redirects:** private IPv4/IPv6, mapped/special/reserved destinations, mixed public/private DNS answers, rebinding pin behavior, unsupported port, same-origin redirect, cross-host redirect, robots-denied redirect path.
4. **Concurrency and politeness:** multiple API/worker pods, concurrent job creation, SKIP LOCKED, one lease per hostname across jobs, minimum delay and Crawl-delay, expiry recovery.
5. **Bounds and retries:** URL/depth/link/job cap, compressed expansion to body limit, response timeout, 429/5xx, integer Retry-After, retry cap, global quota fill and refund on deletion, invalid media type.
6. **Persistence and deletion:** migrations from empty schema, repeated API startup, process/pod replacement, stale lease recovery, concurrent delete and save, cascades, quota accounting.
7. **Frontend:** create, recent jobs, polling/progress, page selection/pagination, validation/network failures, explicit delete, escaped hostile HTML, keyboard/mobile states.
8. **Packaging:** helm lint, schema validation, image build, deployment rollout, replica/PVC health, migration readiness, ClusterIP-only exposure.
9. **Kind smoke:** repeat the loopback UI/API create/read/delete smoke on the verified local context for releases; this worktree passed that flow with a reserved `.invalid` host. Do not claim outbound crawling unless tested against a project-controlled host.
10. **Quality gates:** Jev only via an authorized callable client. SonarQube passed for API, worker, and frontend on `main`; rerun configured analysis after future source changes and report the analyzed revision and gate status. Do not fabricate results or relax quality profiles.

## Open questions and next decisions

- What authentication and rate policy would be required before any network-reachable deployment?
- What backup, restore, PVC expansion, and archive-retention objectives are required for a persistent installation?
- Which robots.txt conformance cases must be added before claiming broader crawler compatibility?
- What user-visible deletion/retention policy is appropriate for archived remote content?

## Decision and next steps

Keep the v1 implementation as a bounded, on-demand crawler using PostgreSQL for durable jobs, frontier, host leases, quota, and archive state. Keep the current DNS/address/redirect safeguards and loopback-only access. Before expanding beyond the local MVP, assign owners to authentication, retention, backup/restore, robots conformance, and storage-capacity decisions. The acceptance cases in this document and the linked verification record remain the release evidence for the local profile.

## 10. References and traceability

- Repository contract and source paths above are primary evidence for implementation behavior.
- [Context Engineering](https://marcelomiyake.com.br/posts/context-engineering/) informs task-specific evidence, traceability, durable context, and keeping human README material distinct from agent instructions.
- [ByteByteGo: Design a Web Crawler](https://bytebytego.com/courses/system-design-interview/design-a-web-crawler) informs the general frontier, politeness, robots, duplicate-discovery, and failure-handling patterns. Its very large scale is explicitly out of scope here.
- The sibling URL shortener is not the source of this project’s persistence decision. Its Cassandra workload differs from the crawler’s transactional local frontier and archive.
- [OpenAPI contract](api/openapi.yaml), [PostgreSQL migration](../database/postgres/migrations/0001_initial.sql), [Helm chart](../deploy/helm/web-crawler/), and service folders are the source paths to inspect when updating this design.

## Glossary

| Term | Meaning |
| --- | --- |
| DDD | Domain-Driven Design; use domain language, invariants, and ownership to shape code and task boundaries. |
| Frontier | Durable set of admitted URLs waiting for a fetch or retry. |
| Idempotency key | Client token that makes a repeated create request return the same job when its normalized request is equivalent. |
| Kind | Local Kubernetes cluster implementation used for this project’s development target. |
| SSRF | Server-Side Request Forgery; this crawler’s risk of being induced to contact private/internal destinations. |
| SKIP LOCKED | PostgreSQL row-lock behavior used to let concurrent workers skip rows another worker currently owns. |
| Jev | TypeSafe structured judgment model for coding-agent task readiness; no Jev call is claimed here. |
