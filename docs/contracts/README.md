# API and data contract catalog

This catalog records the crawler's HTTP and shared PostgreSQL boundaries. [OpenAPI](../api/openapi.yaml), migration files, and the owning service code are authoritative.

> Documentation: [project index](../README.md) · [repository overview](../../README.md)

## Contents

- [Contracts](#contracts)
- [Consumer map](#consumer-map)
- [WebMCP assessment](#webmcp-assessment)
- [Compatibility and security](#compatibility-and-security)
- [Related documentation](#related-documentation)
- [Document lifecycle](#document-lifecycle)
- [AI development disclaimer](#ai-development-disclaimer)

## Contracts

| Contract | Type and authority | Owner | Producer | Consumers |
| --- | --- | --- | --- | --- |
| Crawl Jobs and archive API (`/api/v1`) | REST/JSON; [OpenAPI](../api/openapi.yaml) | `web-crawler` / `crawler-api` | `web-crawler` / `crawler-api` serves the API; request producer is `web-crawler` / `crawler-frontend` | `web-crawler` / `crawler-frontend` submits commands and reads archive responses; other-repository consumers are unknown; none is recorded in the tracked repositories. |
| Durable crawl frontier and page snapshot | PostgreSQL schema; [migrations](../../database/postgres/migrations/) | Repository-owned shared schema; `crawler-api` owns admission/deletion and `crawler-worker` owns execution/snapshot outcomes | `web-crawler` / `crawler-api` admits jobs; `web-crawler` / `crawler-worker` claims and records page results | `web-crawler` / `crawler-api`, `web-crawler` / `crawler-worker`; shared database access is internal only. |
| Host lease and robots policy | PostgreSQL coordination data and worker policy; [System Design](../system-design.md) and `crawler-worker/` | `web-crawler` / `crawler-worker` owns execution policy | `web-crawler` / `crawler-worker` | Other `web-crawler` / `crawler-worker` replicas coordinate through PostgreSQL; API does not own fetch decisions. |
| Frontend same-origin `/api/v1` proxy | NGINX route; [frontend NGINX config](../../crawler-frontend/nginx.conf) | `web-crawler` / `crawler-frontend` owns browser proxy | `web-crawler` / Browser | `web-crawler` / `crawler-api`. |

## Consumer map

The web frontend is the known API client. API and worker share PostgreSQL by design; this is not a public database contract. Other-repository consumers are unknown; none is recorded in the tracked repositories.

## WebMCP assessment

No WebMCP tool contract is implemented. Job creation triggers outbound network access and archive deletion removes persistent page data. Any future read-only status capability needs its own bounded contract and security review; crawled text must remain marked and handled as untrusted.

## Compatibility and security

Job creation is idempotent by normalized request fingerprint. Page content is delivered as `text/plain` with `nosniff`; the frontend renders it as escaped text. Keep API errors categorized and redacted, and keep network safety policy server-side.

## Related documentation

- [System Design](../system-design.md)
- [Project README](../../README.md)
- [Documentation index](../README.md)

## Document lifecycle

This contract catalog is maintained as Markdown and links to the implementation-owned interface definitions. It has no independent software build, deployment, or undeployment lifecycle. Review it when its linked contracts or consumers change.


## AI development disclaimer

> **AI development disclaimer:** This project was built entirely with GPT-6 Luna at Max effort as a proof of concept exploring how low-cost AI plans can be useful when paired with disciplined harness and loop engineering. This is project-owner attribution; repository contents do not independently verify runtime model metadata. Review AI-generated design and code before relying on them.
