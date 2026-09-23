# Kubernetes CPU and memory budgets

These chart-configured values apply per pod/container in the local Kind deployment. They are not measured consumption or production sizing claims.


> Project documentation index: [Documentation index](README.md)


## Workload budgets

| Pod/workload and container | CPU request | Memory request | CPU limit | Memory limit | Source |
| --- | ---: | ---: | ---: | ---: | --- |
| `web-crawler-api` / `api` | 100m | 128Mi | 500m | 384Mi | [Chart values](../deploy/helm/web-crawler/values.yaml) |
| `web-crawler-api` / init `wait-for-postgres` | 10m | 16Mi | 100m | 64Mi | [Chart helper](../deploy/helm/web-crawler/templates/_helpers.tpl) |
| `web-crawler-worker` / `worker` | 100m | 128Mi | 500m | 512Mi | [Chart values](../deploy/helm/web-crawler/values.yaml) |
| `web-crawler-worker` / init `wait-for-postgres` | 10m | 16Mi | 100m | 64Mi | [Chart helper](../deploy/helm/web-crawler/templates/_helpers.tpl) |
| `web-crawler-frontend` / `frontend` | 25m | 32Mi | 100m | 128Mi | [Chart values](../deploy/helm/web-crawler/values.yaml) |
| `web-crawler-postgres` / `postgres` | 250m | 512Mi | 1 | 1Gi | [Chart values](../deploy/helm/web-crawler/values.yaml) |

Each API and worker pod has the listed init container with explicit CPU/memory requests and limits. PostgreSQL separately requests a 10Gi PVC. No sidecars are defined.

## Kubernetes resource management practice

Set CPU and memory `requests` and `limits` on every container in every Pod, including init containers and sidecars. Requests guide scheduling and reserve baseline capacity; CPU limits may throttle, and memory limits can trigger OOM termination. Measure representative usage, leave startup/burst headroom, monitor throttling and restarts, and right-size deliberately. Treat the listed values as local development defaults, not production sizing guidance. PVC storage requests are separate from container budgets.

## Build, use, and persistence

The [root README](../README.md) documents build, deploy, use, and undeploy commands with their persistent-data effects. The [Helm chart guide](../deploy/helm/web-crawler/README.md) is the deployment owner. This resource inventory is configuration guidance and does not itself deploy workloads.
