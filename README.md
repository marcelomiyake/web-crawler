# Web Crawler

A local, on-demand HTML archive. Enter seed URLs and exact hostnames, create a bounded crawl, monitor progress, read saved source as plain text, and delete the archive when finished. The UI is intended for loopback port-forwarding into a local Kind cluster; it is not a public crawler service.

A complete document map is available in the [Documentation index](docs/README.md).

## Contents

- [Project status](#project-status)
- [Architecture](#architecture)
- [Safety defaults](#safety-defaults)
- [Local Kind workflow](#local-kind-workflow)
- [Development checks](#development-checks)
- [SonarQube projects](#sonarqube-projects)
- [Project documents](#project-documents)
- [Build, use, and resource prerequisites](#build-use-and-resource-prerequisites)
- [Undeploy](#undeploy)
- [Screenshots](#screenshots)
- [AI development disclaimer](#ai-development-disclaimer)

## Project status

The v1 application, tests, PostgreSQL migration, Dockerfiles, and Helm chart are implemented in this repository. Verification evidence and remaining limits are recorded in [the system design](docs/system-design.md). This is an early local tool, not a production-ready public crawler: PostgreSQL is a single local failure domain, there is no authentication, backup, search, JavaScript rendering, or scheduled recrawl.

## Architecture

| Service | Stable ID | Folder | Responsibility | Local replicas |
| --- | --- | --- | --- | ---: |
| Vue frontend | `web-crawler-frontend` | [crawler-frontend/](crawler-frontend/) | Accessible UI, escaped page source, same-origin API proxy | 2 |
| Rust API | `web-crawler-api` | [crawler-api/](crawler-api/) | Job creation and idempotency, status, page reads, deletion | 2 |
| Rust worker | `web-crawler-worker` | [crawler-worker/](crawler-worker/) | Durable frontier, robots rules, safe fetches, snapshots | 2 |
| PostgreSQL | `web-crawler-postgres` | [database/postgres/migrations/](database/postgres/migrations/) | Durable job/frontier/host state, robots cache, archive | 1 StatefulSet pod |

Each application service is built and deployed separately. Each service folder owns a `README.md`, `AGENTS.md`, Dockerfile, source, and tests. The three app Deployments each use at least two replicas. The local PostgreSQL StatefulSet has one replica, so these app replicas do not make the system highly available.

The API and worker share PostgreSQL intentionally for v1. Database transactions, URL uniqueness, global host leases, and `FOR UPDATE SKIP LOCKED` preserve crawl state without Redis or a broker. See [database trade-offs](docs/system-design.md#7-data-model-and-database-recommendation).

## Safety defaults

- 500 unique URLs per job, depth 3, 2 MiB decompressed body per response, 10-second end-to-end fetch deadline, five redirects, three page attempts, and at most 1,000 link candidates per page.
- Exact host allowlist; HTTP/HTTPS default ports only; validate all resolved addresses and pin a public destination; disable implicit proxies and automatic redirects.
- `robots.txt` is checked before page fetches; requests share one hostname lease across jobs and worker pods. Robots rules are cached for one hour. Cross-host and cross-origin redirects are rejected.
- A 6 GiB global body budget leaves headroom on the 10 GiB Kind volume for PostgreSQL indexes, WAL, and metadata. New jobs receive HTTP 507 when the budget is exhausted; an in-flight page that reaches it fails safely. Jobs remain until manually deleted.
- Saved responses are returned as `text/plain` and rendered as escaped text. Full page paths, query strings, page bodies, cookies, and credentials are not written to logs.

## Local Kind workflow

These commands target the existing local cluster named `kind` and context `kind-kind`. Verify the context and cluster before running them. Do not use these steps against a remote context.

~~~sh
kubectl config current-context
kind get clusters
kubectl --context kind-kind cluster-info
kubectl --context kind-kind get nodes -o wide
kubectl --context kind-kind get storageclass

docker build --tag web-crawler-api:dev --file crawler-api/Dockerfile .
docker build --tag web-crawler-worker:dev --file crawler-worker/Dockerfile .
docker build --tag web-crawler-frontend:dev --file crawler-frontend/Dockerfile .
kind load docker-image web-crawler-api:dev web-crawler-worker:dev web-crawler-frontend:dev --name kind

kubectl --context kind-kind get namespace web-crawler >/dev/null 2>&1 || \
  kubectl --context kind-kind create namespace web-crawler
if ! kubectl --context kind-kind --namespace web-crawler get secret web-crawler-postgres >/dev/null 2>&1; then
  kubectl --context kind-kind --namespace web-crawler create secret generic web-crawler-postgres \
    --from-literal=password="$(openssl rand -hex 32)"
fi

helm --kube-context kind-kind upgrade --install web-crawler deploy/helm/web-crawler \
  --namespace web-crawler --create-namespace --wait --timeout 8m
kubectl --context kind-kind --namespace web-crawler get deployments,statefulsets,pods,pvc

# In a separate terminal; this binds only to loopback.
kubectl --context kind-kind --namespace web-crawler \
  port-forward --address 127.0.0.1 service/web-crawler-frontend 8080:8080
~~~

Open `http://127.0.0.1:8080`. The chart does not create a Secret from committed values. Keep the Secret and PostgreSQL PVC in the named namespace. Deleting that namespace deletes the local archive; no backup or restore path is included.

## Development checks

~~~sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings

cd crawler-frontend
npm ci
npm run typecheck
npm run lint
npm run test:unit
npm run build
npm run test:e2e
~~~

The Rust workspace suite includes a PostgreSQL integration test and requires `TEST_DATABASE_URL`. Run it only after starting an isolated disposable database as shown here; the shell trap removes the temporary container whether the tests pass or fail:

~~~sh
docker run --detach --rm --name web-crawler-test-db \
  --env POSTGRES_USER=crawler_test \
  --env POSTGRES_PASSWORD=test-only \
  --env POSTGRES_DB=web_crawler_test \
  --publish 127.0.0.1:55439:5432 postgres:17-alpine
trap 'docker stop web-crawler-test-db >/dev/null 2>&1 || true' EXIT
until docker exec web-crawler-test-db pg_isready -U crawler_test -d web_crawler_test; do sleep 1; done
TEST_DATABASE_URL='postgres://crawler_test:test-only@127.0.0.1:55439/web_crawler_test' cargo test --workspace --locked
~~~

Never point destructive tests at Kind’s persistent volume. The Playwright UI suite stubs the API; it is not evidence of a live outbound crawl.

## SonarQube projects

The local SonarQube server has three separately scoped projects: `web-crawler-api`, `web-crawler-worker`, and `web-crawler-frontend`. Rust scans include the shared domain and configuration crates. Each scan imports Clippy findings and Rust LCOV coverage; the Vue unit suite generates JavaScript/Vue LCOV coverage.

With a disposable PostgreSQL test database running, generate the reports and run the configured quality gates:

~~~sh
cargo clippy --workspace --all-targets --locked --message-format=json -- -D warnings > target/sonar-clippy.json
TEST_DATABASE_URL='postgres://crawler_test:test-only@127.0.0.1:55439/web_crawler_test' cargo llvm-cov --workspace --all-targets --locked --lcov --output-path target/sonar-rust-lcov.info
python3 .sonar/normalize-rust-lcov.py
(cd crawler-frontend && npm run test:unit)
SONAR_HOST_URL='http://127.0.0.1:9000' ./.sonar/scan.sh
~~~

Export `SONAR_TOKEN` in the environment before running the scanner script. It uses the official scanner container and waits for each configured Quality Gate. Never commit the token or place it in a command argument. The Quality Gate belongs to the SonarQube server and may evolve independently of this repository.

The latest recorded analysis passed all three project gates on `main` (2026-09-24): API coverage 95.4%, worker coverage 81.8%, and frontend coverage 85.4%. Each project had zero open findings and zero duplicated lines. See the [verification record](docs/system-design.md#9-verification-and-acceptance) for the scan conditions and scope.

## Project documents

- [System design](docs/system-design.md) — requirements, boundaries, API, schema, safety, trade-offs, operations, and acceptance.
- [OpenAPI contract](docs/api/openapi.yaml) — API paths and payloads.
- [Agent guidance](AGENTS.md) — DDD/Jev, quality gates, testing, and local Kubernetes workflow.
- [Frontend design notes](crawler-frontend/DESIGN.md) — interaction model and accessibility.

## Build, use, and resource prerequisites

- **Build and deploy:** Rust/Cargo, Node.js/npm, Docker, Kind, kubectl, Helm, and an existing local cluster named `kind` with the `standard` StorageClass.
- **Use:** a browser, a ready local deployment, and loopback port-forward access. Crawl only public hosts you are authorized to fetch; the archive is synthetic/local and has no backup workflow.

## Undeploy

Stop the frontend port-forward with Ctrl-C and uninstall the release while retaining the PostgreSQL PVC:

```sh
helm --kube-context kind-kind uninstall web-crawler --namespace web-crawler
```

The chart retains the database claim. Deleting the namespace removes the local archive and PVC. CPU and memory requests and limits for all regular and init containers are documented in [Kubernetes resource budgets](docs/kubernetes-resources.md).

## Screenshots

![Web Crawler local archive showing the completed example.com crawl and HTTP 200 page](docs/assets/screenshots/web-crawler.png)


## AI development disclaimer

> **AI development disclaimer:** This project was built entirely with GPT-6 Luna at Max effort as a proof of concept exploring how low-cost AI plans can be useful when paired with disciplined harness and loop engineering. This is project-owner attribution; repository contents do not independently verify runtime model metadata. Review AI-generated design and code before relying on them.
