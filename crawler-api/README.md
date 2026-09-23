# Crawler API

- **Stable service ID:** `web-crawler-api`
- **Ownership:** accepts crawl-job commands, applies URL/host/limit validation, serves job state and page metadata/content, and deletes job-owned data.
- **Runtime:** Rust, Axum, SQLx, PostgreSQL. Kubernetes runs this service in its own Deployment with at least two replicas.

> Documentation: [project index](../docs/README.md) · [repository overview](../README.md)

## Contents

- [Authoritative contracts](#authoritative-contracts)
- [Run locally](#run-locally)
- [Checks](#checks)
- [Component ownership, prerequisites, and lifecycle](#component-ownership-prerequisites-and-lifecycle)
- [AI development disclaimer](#ai-development-disclaimer)

## Authoritative contracts

- [Root system design](../docs/system-design.md) — product requirements, safety policy, state transitions, and operational boundaries.
- [OpenAPI](../docs/api/openapi.yaml) — external HTTP contract.
- [PostgreSQL migrations](../database/postgres/migrations/) — durable data constraints. The API binary applies embedded migrations before listening; concurrent API replicas use SQLx’s PostgreSQL migration lock.

The API and worker intentionally share PostgreSQL in v1. The API owns job creation/idempotency and deletion behavior; the worker owns frontier claims, robots decisions, fetch and snapshot writes. Neither service may bypass the OpenAPI, SQL constraints, or migration history.

## Run locally

Start a disposable PostgreSQL database and set `DATABASE_URL`, then:

~~~sh
cargo run --locked --package crawler-api
~~~

The API listens on `0.0.0.0:8081`. `GET /health/live` checks the process; `GET /health/ready` checks PostgreSQL. Do not bind or expose it publicly. The intended user entry point is the frontend through a loopback port-forward.

## Checks

~~~sh
cargo fmt --all --check
cargo clippy --locked --package crawler-api --all-targets -- -D warnings
TEST_DATABASE_URL='postgres://crawler_test:test-only@127.0.0.1:55439/web_crawler_test' cargo test --locked --package crawler-api
~~~

Start the isolated disposable PostgreSQL container from the root [README test instructions](../README.md#development-checks) before this suite. Never point integration tests at Kind’s persistent PVC.

## Component ownership, prerequisites, and lifecycle

- **Owner:** `web-crawler` / `crawler-api`.
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
