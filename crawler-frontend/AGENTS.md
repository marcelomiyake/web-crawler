# Frontend service guidance

> Human guide: [README.md](../README.md) · [Documentation index](../docs/README.md)

- Service ID: `web-crawler-frontend`. Follow root [AGENTS.md](../AGENTS.md), [system design](../docs/system-design.md), the [OpenAPI contract](../docs/api/openapi.yaml), and [design notes](DESIGN.md).
- Use OpenDesign to refine substantial frontend flows when that tool is available. Review the result before translation; do not add design-time tools as runtime dependencies. If unavailable, record that limitation and keep the handoff in `DESIGN.md`.
- Keep validation messages accessible and make empty, loading, success, validation, network, and failure states explicit. Preserve keyboard focus, reduced-motion behavior, and responsive layouts.
- Treat all remote text as hostile. Show archive content only through escaped Vue interpolation or `textContent`; never `v-html`, `innerHTML`, active embeds, previews, or fetched subresources.
- Do not persist page content, full destination URLs, credentials, or crawl responses in browser storage or analytics. Store only minimal recent job identifiers.
- Keep API calls same-origin under `/api/v1`; do not enable wildcard CORS or expose the API directly in the UI. Keep NGINX internal and port-forward only the frontend in Kind.
- Do not expose crawl creation or archive deletion as WebMCP tools. Any future read-only status tool needs a separately reviewed contract, bounded output, untrusted-content annotation, cancellation, and teardown coverage.
- Run `npm ci`, `npm run typecheck`, `npm run lint`, `npm run test:unit`, `npm run build`, and `npm run test:e2e`. Keep the mocked browser tests distinct from a live Kind/system E2E.
- Root instructions define DDD/Jev, SonarQube quality/duplication, Conventional Commits, Kubernetes, and Kind gates.
