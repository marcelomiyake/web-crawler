# Crawler frontend

- **Stable service ID:** `web-crawler-frontend`
**Ownership:** Vue 3 interface for creating bounded jobs, tracking progress, reading archived source as escaped text, and deleting a job. Kubernetes runs this service in its own Deployment with at least two replicas.

> Documentation: [project index](../docs/README.md) · [repository overview](../README.md)

## Contents

- [Authoritative contracts](#authoritative-contracts)
- [Local development](#local-development)
- [Checks](#checks)
- [Browser agent support (WebMCP)](#browser-agent-support-webmcp)
- [Component ownership, prerequisites, and lifecycle](#component-ownership-prerequisites-and-lifecycle)
- [AI development disclaimer](#ai-development-disclaimer)

## Authoritative contracts

- [Root system design](../docs/system-design.md) — product workflow, safety limits, and service boundaries.
- [OpenAPI](../docs/api/openapi.yaml) — API request and response shapes.
- [Design notes](DESIGN.md) — implemented visual language, interaction states, accessibility, and responsive behavior.

NGINX serves this static application and proxies `/api/` to the internal API ClusterIP Service. The frontend keeps only up to five recent job IDs in local storage; page content stays in PostgreSQL and is fetched on demand.

## Local development

~~~sh
npm ci
npm run dev
~~~

Vite serves the UI on port 5173 and proxies `/api` to `127.0.0.1:8081` for local development.

## Checks

~~~sh
npm run typecheck
npm run lint
npm run test:unit
npm run build
npm run test:e2e
~~~

The Playwright suite stubs the API for repeatable UI checks. A live local cluster smoke is a separate acceptance activity; the current suite does not claim to validate real outbound crawling.

The frontend-only Lighthouse and SEO metadata audit is recorded in [verification evidence](../docs/verification/lighthouse.md); it does not prove crawl safety or a live backend flow.

## Browser agent support (WebMCP)

WebMCP is not enabled for the crawler console. Creating a job triggers outbound network requests, and deleting a job permanently removes archived page data. Crawled page titles and snapshots are also untrusted. Keep these actions in the reviewed UI flow; a future read-only status tool would need a separate contract, bounded outputs, and explicit tests before it is registered.

## Component ownership, prerequisites, and lifecycle

- **Owner:** `web-crawler` / `crawler-frontend`.
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
