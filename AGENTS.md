# Agent guidance

## Contents

- [Repository state and sources of truth](#repository-state-and-sources-of-truth)
- [Service ownership and layout](#service-ownership-and-layout)
- [Domain-driven design for agent work](#domain-driven-design-for-agent-work)
- [Agent job quality gate (DDD + Jev)](#agent-job-quality-gate-ddd-jev)
- [Frontend design with OpenDesign](#frontend-design-with-opendesign)
- [Crawler and data safety](#crawler-and-data-safety)
- [Tests and verification](#tests-and-verification)
- [SonarQube quality and duplication gate](#sonarqube-quality-and-duplication-gate)
- [Kubernetes and local Kind](#kubernetes-and-local-kind)
- [Commit messages](#commit-messages)

## Repository state and sources of truth

This repository now contains an implemented local v1. Code and verification details can change independently of this guide. The last committed baseline before implementation was `61b52ab0292c65ff6ac94ec84d55f001272dc938` (2026-09-23).

- [README.md](README.md) is the human entry point and local runbook.
- [docs/system-design.md](docs/system-design.md) owns requirements, decisions, data ownership, safety limits, and operations.
- [docs/api/openapi.yaml](docs/api/openapi.yaml) owns the HTTP contract.
- [database/postgres/migrations/](database/postgres/migrations/) owns the versioned PostgreSQL schema.
- `crawler-api/`, `crawler-worker/`, and `crawler-frontend/` own their service code, tests, Dockerfiles, `README.md`, and `AGENTS.md`.
- [deploy/helm/web-crawler/](deploy/helm/web-crawler/) owns local Kubernetes resources. Do not create duplicate raw manifests.
- `docs/system-design.md` is the sole system-design document; keep it synchronized with implemented behavior.

Distinguish implemented behavior, environment observations, proposals, and open questions. Cite repository paths/symbols and the analyzed revision. Do not claim a test, image build, SonarQube result, or deployment succeeded unless its current output is inspected.

## Service ownership and layout

~~~text
crawler-api/                     Rust API, tests, Dockerfile, README.md, AGENTS.md
crawler-worker/                  Rust worker, tests, Dockerfile, README.md, AGENTS.md
crawler-frontend/                Vue 3/TypeScript, unit/E2E tests, Dockerfile, README.md, AGENTS.md
crates/crawler-domain/           Shared URL/host safety policy and domain values
crates/crawler-config/           Shared PostgreSQL connection configuration
database/postgres/migrations/    Versioned PostgreSQL schema
docs/api/openapi.yaml            HTTP contract
deploy/helm/web-crawler/         Local Kind application chart
docs/system-design.md            Canonical architecture and requirements
~~~

Stable service IDs are `web-crawler-api`, `web-crawler-worker`, and `web-crawler-frontend`. Each application has its own image, Deployment/pods, folder, and service-owned docs. Keep at least two replicas for each app. PostgreSQL is a single StatefulSet pod for local Kind, not an application microservice or an HA database.

The API and worker share PostgreSQL in v1. The API owns job admission, idempotency, read contract, and user deletion. The worker owns durable frontier execution, host leases, robots decisions, outbound fetch, and snapshots. These are deployment boundaries with explicit shared persistence; do not describe them as independently owned databases. Migrations remain shared repository ownership.

## Domain-driven design for agent work

- Use DDD to clarify use cases, ownership, invariants, terminology, and consistency boundaries; avoid patterns as ceremony. Keep Crawl Job, Seed URL, Allowed Host, Frontier URL, Host Lease, Robots Policy, and Page Snapshot consistent.
- Discover bounded contexts from coherent use cases, data ownership, language, and consistency needs. Do not infer that a folder, pod, agent, or service is automatically a bounded context.
- For domain changes, record command/use case, invariants, lifecycle, authoritative owner, contracts, consistency, failures, and open decisions. Use aggregates/value objects only where they enforce an identified rule.

## Agent job quality gate (DDD + Jev)

For each non-trivial implementation/delegation, before dependent work starts, define:

- Actor, business outcome, candidate context(s), domain language, and invariants.
- Repository evidence/revision; separate confirmed facts from proposals/assumptions.
- Scope, exclusions, owners, dependencies, and cross-context contracts.
- Deliverables and observable acceptance checks.
- Safety/data/authorization boundaries, failure modes, stop conditions, and open questions with an owner or discovery action.

When an authorized Jev client/API is available, ask one independent Score question for each of the four readiness dimensions with the same minimal evidence-backed brief. Jev scores readiness; it does not write the task or certify its implementation. Inspect score distribution and confidence, not only a scalar.

| Dimension | 0 | 1 | 2 | Ready level 3 |
| --- | --- | --- | --- | --- |
| Domain fit | Contradictory or unsafe | Actor/outcome/context missing | Main context named; language/rules incomplete | Actor, outcome, context, terms, and open policy explicit |
| Scope/evidence | Unsupported or unbounded | Scope/source/dependencies missing | Main scope clear; some evidence/owners/contracts absent | Scope, exclusions, sources, owners, dependencies, contracts explicit |
| Deliverable/acceptance | No handoff | Activity without verifiable result | Deliverable exists; acceptance partly subjective | Traceable deliverables and observable verification |
| Risk/decision ownership | Material risk hidden | Safety/data/owner omitted | Main risks known; recovery/stop/owner incomplete | Limits, failures, owners, authorization, stop conditions explicit |

Local initial-readiness guardrails are score ≥2.5 and probability across levels 0–2 ≤0.20 on each dimension; do not average away a weak critical dimension. These are local guardrails, not Jev defaults. If confidence is low or a dimension fails, improve evidence and re-score with at most two focused revisions; then stop dependent work and surface the missing decision. If Jev is unavailable or unauthorized, use the rubric manually and state Jev was not called. Never fabricate scores/confidence or send page bodies, secrets, or unrelated private data. Recheck the official [Score docs](https://docs.typesafe.ai/primitives/score), [Jev for coding agents](https://docs.typesafe.ai/introduction/coding-agents), and [composite scoring](https://docs.typesafe.ai/patterns/composite-scoring) before integration.

## Frontend design with OpenDesign

- Use [OpenDesign](https://github.com/nexu-io/open-design) for substantial frontend design when the tool is available: refine the flow, review a rendered prototype, then implement the accepted design.
- Keep the project handoff in [crawler-frontend/DESIGN.md](crawler-frontend/DESIGN.md), including visual language, accessibility, responsive behavior, and interaction states.
- Treat design prototypes as design-time artifacts, not API contracts, functional tests, or runtime dependencies. Review generated assets before adoption.
- If OpenDesign is unavailable, record the limitation and continue with a deliberate documented design; do not claim an OpenDesign review occurred.

## Crawler and data safety

- Server validation is authoritative. Accept HTTP/HTTPS default ports only; reject URL credentials, IP literal hosts, malformed/overlong input, and exact-host allowlist mismatches. Normalize with the shared maintained `url` parser.
- Before every connection resolve all IPv4/IPv6 answers, reject the request if an answer is non-public/special, and pin the selected validated address while retaining TLS hostname checks. Disable proxies and automatic redirects.
- Redirects must remain on the same hostname and origin in v1; recheck robots rules and destination safety on each hop. Do not forward credentials. The worker fails closed when robots policy cannot be fetched or parsed.
- Robots, pages, and redirect hops use a global per-host lease. Maintain at least one second between requests; supported `Crawl-delay` values are capped at 60 seconds. Lease coordination and frontier state belong in PostgreSQL.
- Preserve the hard bounds: 500 unique URLs/job, depth 3, 2 MiB decompressed body, 10-second fetch deadline, five redirects, 1,000 links/page, 4 KiB URL, and three page attempts. The 6 GiB archive-body budget is on a 10 Gi local PVC. Lower per-job settings are allowed; do not raise hard limits without a design/security review.
- Parse HTML/XHTML only. UI displays archived content as escaped text; never use `v-html`, `innerHTML`, iframe, or active page subresources.
- Avoid logging full paths/query strings, headers, bodies, cookies, or credentials. Never commit database credentials or embed them in frontend assets or images.

## Tests and verification

Maintain and accurately label each testing level:

- **Unit:** URL normalization and IP policy; robots rules, retries, limits, transitions; Vue states and inert escaped text.
- **Integration:** API/migrations/worker against an isolated disposable PostgreSQL database; uniqueness, idempotency, `SKIP LOCKED`, host leases, retries, quota, cascade deletion, and persistence. Never run destructive tests against Kind’s persistent PVC.
- **End-to-end:** Playwright browser flow for create/progress/source/delete/errors. Distinguish mocked API E2E from actual Kind smoke. For outbound Kind crawl, use only an explicitly project-controlled public host; use in-process fixtures for private IP and redirect cases. Never add a deployed test-only SSRF bypass.

Run the narrowest checks that prove a change. At repository root, intended checks are:

~~~sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings > target/sonar-clippy.json
# Start the disposable PostgreSQL container shown in README.md, then:
TEST_DATABASE_URL='postgres://crawler_test:test-only@127.0.0.1:55439/web_crawler_test' cargo test --workspace --locked
TEST_DATABASE_URL='postgres://crawler_test:test-only@127.0.0.1:55439/web_crawler_test' cargo llvm-cov --workspace --all-targets --locked --lcov --output-path target/sonar-rust-lcov.info
python3 .sonar/normalize-rust-lcov.py

cd crawler-frontend
npm ci
npm run typecheck
npm run lint
npm run test:unit
npm run build
npm run test:e2e
cd ..

helm lint deploy/helm/web-crawler
helm template web-crawler deploy/helm/web-crawler --namespace web-crawler
~~~

Do not label unit tests integration/E2E. Report the exact command and result; tests alone do not prove crawler safety, Sonar cleanliness, data recovery, or deployment readiness.

## SonarQube quality and duplication gate

- SonarQube project scopes are checked in under `.sonar/api.properties`, `.sonar/worker.properties`, and `.sonar/frontend.properties`. API and worker scans include the shared domain/config crates so each service analysis covers its compiled behavior; frontend scans the Vue app plus its unit and browser tests.
- Rust scans import `target/sonar-clippy.json` and service-scoped LCOV reports generated from `target/sonar-rust-lcov.info`; regenerate them with the commands above against a disposable PostgreSQL database before analysis. Vue unit tests generate the frontend LCOV report. Run all three analyses with `.sonar/scan.sh` after setting `SONAR_HOST_URL` and `SONAR_TOKEN` in the environment. Do not pass or print the token as a command-line value.
- Analyze API, worker, and frontend as separately owned projects with relevant coverage. Require a meaningful configured Quality Gate to pass, including duplication conditions; inspect bugs, vulnerabilities, hotspots, code smells, and duplicated code. An empty gate is not a quality pass.
- Before reporting results, inspect the project, branch, analyzed revision, issues, duplication metrics, and the server-side Quality Gate conditions/status. If the project, branch, or revision does not match the checkout, report the result as stale/incomplete.
- Fix real duplication or findings without weakening profiles, thresholds, or exclusions. Share code only when behavior and ownership are genuinely shared.
- Use configured SonarQube integration/scanner only when available. If scanner, authorization, project, or current results are unavailable, report the gate incomplete; never expose tokens.

## Kubernetes and local Kind

- Before any cluster operation, inspect current context, `kind get clusters`, `kubectl cluster-info`, nodes, StorageClass, and tool versions. The 2026-09-24 observed context is `kind-kind`, cluster `kind`, one v1.37.0 node, `standard` local-path storage without expansion, and Helm v4.3.0; recheck before use.
- For this local project use only context `kind-kind` and cluster `kind`. If they do not match, stop before applying. Do not create, switch, upgrade, or delete clusters/contexts incidentally.
- The chart is source of truth. Keep namespace `web-crawler`, an external Kubernetes Secret `web-crawler-postgres`, separate internal ClusterIP Services, API/worker/frontend Deployments (≥2 replicas each), and one PostgreSQL StatefulSet/PVC. API and worker init containers wait for PostgreSQL readiness; the API binary applies embedded versioned migrations before listening, so no separate migration Job is required.
- App containers should run non-root with resource requests/limits, probes, read-only root filesystems, dropped capabilities, and no service-account token. Do not expose Ingress, NodePort, or a public load balancer. Port-forward frontend only to `127.0.0.1`.
- A successful chart render or install is not a working crawl. Verify replicas, PVC, API readiness, migrations, UI flow, and only a controlled crawl target before reporting a Kind smoke.
- Deleting namespace `web-crawler` deletes its PVC/archive. There is no backup/restore or database HA guarantee.

## Commit messages

Follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) for every commit. Use `type[optional scope]: description`; place `!` after type/scope for breaking changes and include a `BREAKING CHANGE:` footer. Add a body/other footers where helpful.
