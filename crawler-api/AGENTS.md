# API service guidance

> Human guide: [README.md](../README.md) · [Documentation index](../docs/README.md)

- Service ID: `web-crawler-api`. Follow the root [AGENTS.md](../AGENTS.md) and [system design](../docs/system-design.md).
- The HTTP contract is [OpenAPI](../docs/api/openapi.yaml). Validate every request on the server; browser checks are only usability hints.
- This service owns job creation/idempotency and API-mediated deletion. PostgreSQL migrations remain shared repository ownership under `database/postgres/migrations/`; preserve forward-only, reviewable migration history.
- Keep job/URL limits, normalized host exactness, and stable redacted error categories aligned with the contract. Never log credentials, body data, or full paths/query strings.
- Content is returned as `text/plain; charset=utf-8` with `nosniff`. Never add an HTML-serving endpoint for crawled source.
- Do not add routes that expose database diagnostics, remote fetches, arbitrary URL proxies, or public access. Keep CORS disabled; the frontend is same-origin proxied.
- Test domain validation with unit tests and storage behavior with a disposable PostgreSQL integration database. Never run destructive tests against Kind’s archive PVC.
- Run `cargo fmt --all --check`, `cargo clippy --locked --package crawler-api --all-targets -- -D warnings`, and `TEST_DATABASE_URL=... cargo test --locked --package crawler-api` against an isolated disposable database before handoff. Root instructions define Jev, SonarQube, Conventional Commit, Helm, and Kind gates.
