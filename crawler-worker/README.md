# Crawler worker

- **Stable service ID:** `web-crawler-worker`
**Ownership:** durable frontier claims, robots policy, host leases/pacing, safe outbound fetch, bounded HTML parsing, and page snapshot persistence. Kubernetes runs this service in its own Deployment with at least two replicas.

> Documentation: [project index](../docs/README.md) · [repository overview](../README.md)

## Contents

- [Authoritative contracts](#authoritative-contracts)
- [Run locally](#run-locally)
- [Checks](#checks)
- [Component ownership, prerequisites, and lifecycle](#component-ownership-prerequisites-and-lifecycle)
- [AI development disclaimer](#ai-development-disclaimer)

## Authoritative contracts

- [Root system design](../docs/system-design.md) — limits, safety, failure, and storage policy.
- [OpenAPI](../docs/api/openapi.yaml) — user-facing job and archive contract.
- [PostgreSQL migrations](../database/postgres/migrations/) — shared durable queue and lease model.

The API and worker share PostgreSQL intentionally. The worker does not expose an inbound crawl API. It polls the durable frontier and exposes only liveness/readiness probes on port 8082. Claims use database row locks and globally shared host leases, so adding pods does not allow concurrent fetches to the same hostname.

## Run locally

With a disposable PostgreSQL database configured through `DATABASE_URL` (or `DB_HOST`, `DB_PORT`, `DB_NAME`, `DB_USER`, and `DB_PASSWORD`), and after migrations have been applied:

~~~sh
cargo run --locked --package crawler-worker
~~~

Default behavior: two polling tasks, 10-second request deadline, 2 MiB decompressed body limit, three page attempts, a 500 URL / depth-3 job cap, robots cache, and at least one second between requests per hostname. The worker validates all DNS answers and pins the connection to a public answer; redirects must remain on the same hostname and origin and pass robots rules.

## Checks

~~~sh
cargo fmt --all --check
cargo clippy --locked --package crawler-worker --all-targets -- -D warnings
cargo test --locked --package crawler-worker
~~~

Use controlled local fixtures for parser and decision tests. Never weaken the public-address guard or ship test-only SSRF exceptions in Helm values.

## Component ownership, prerequisites, and lifecycle

- **Owner:** `web-crawler` / `crawler-worker`.
- **API, event, and data contract owners/producers/consumers:** see the [contract catalog](../docs/contracts/README.md) for each authoritative interface.
- **Parent architecture:** [System Design](../docs/system-design.md).

### Build prerequisites

Use this component’s pinned toolchain and lockfile/wrapper. The supported versions and complete local build environment are listed in the [root README](../README.md).

### Use prerequisites

This component is used as part of the parent system. Start its required local dependencies and use the supported local access path described in the [root README](../README.md).

### Build, verify, deploy, undeploy, and use

Build, run, and verification commands for this component are documented above. There is no independent release lifecycle for this component.
The parent Helm release owns deployment and removal; follow the [chart guide](../deploy/helm/web-crawler/README.md) and [root deployment lifecycle](../README.md). Uninstall removes application workloads while retained PVCs keep their data; deleting the namespace or claims purges persistent data.

[Documentation index](../docs/README.md)


## AI development disclaimer

> **AI development disclaimer:** This project was built entirely with GPT-6 Luna at Max effort as a proof of concept exploring how low-cost AI plans can be useful when paired with disciplined harness and loop engineering. This is project-owner attribution; repository contents do not independently verify runtime model metadata. Review AI-generated design and code before relying on them.
