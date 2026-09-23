# Worker service guidance

> Human guide: [README.md](../README.md) · [Documentation index](../docs/README.md)

- Service ID: `web-crawler-worker`. Follow root [AGENTS.md](../AGENTS.md) and [system design](../docs/system-design.md).
- The Rust worker owns frontier claim/recovery, robots policy, DNS/IP checks, redirect rules, response limits, extraction, and snapshot writes. Keep crawler domain language consistent: Crawl Job, Allowed Host, Frontier URL, Host Lease, Robots Policy, Page Snapshot.
- Resolve and inspect every A/AAAA result, reject the request if any answer is prohibited, and pin an approved result to the HTTP connection. Disable implicit proxies and automatic redirects. Reject cross-host and cross-origin redirects; recheck same-origin robots rules on each hop.
- Robots, pages, and redirects all use the shared hostname lease and rate schedule. Do not sleep while holding a database transaction. Do not mark a job complete while queued work or live claims remain.
- Stream decompressed response data under the hard body cap. Extract links with exact allowlist membership; never execute JavaScript or load page subresources.
- Log only hostname, identifiers, category, sizes, and timings. Do not log full URLs, query strings, bodies, cookies, or credentials.
- Add unit coverage for URL/IP/robots/retry policy and isolated PostgreSQL integration coverage for claims, leases, frontier limits, storage budget, deletion races, and restart recovery. Do not run destructive work against Kind’s PVC.
- Run `cargo fmt --all --check`, `cargo clippy --locked --package crawler-worker --all-targets -- -D warnings`, and `cargo test --locked --package crawler-worker` before handoff. Root instructions define Jev, SonarQube, Conventional Commit, Helm, and Kind gates.
