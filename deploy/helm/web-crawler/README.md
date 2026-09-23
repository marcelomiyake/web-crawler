# Web Crawler Helm chart

This chart deploys the API, worker, frontend, and PostgreSQL into the local `web-crawler` namespace. The API and worker share PostgreSQL by design. No public ingress is created.

> Documentation: [project index](../../../docs/README.md) · [repository overview](../../../README.md)

## Contents

- [Build and use prerequisites](#build-and-use-prerequisites)
- [Prerequisites and deploy](#prerequisites-and-deploy)
- [Resource budgets and persistence](#resource-budgets-and-persistence)
- [Undeploy and use](#undeploy-and-use)
- [AI development disclaimer](#ai-development-disclaimer)

## Build and use prerequisites

- **Build/deploy:** use the root project README for source-image build commands and the chart instructions below for the Helm release. Required tools are Docker, the supported local Kubernetes cluster/context, kubectl, and Helm.
- **Use:** the cluster release must be ready; use the loopback browser/port-forward instructions in the root README. See the resource table for each pod container budget.


## Prerequisites and deploy

Use the existing Kind cluster/context `kind` / `kind-kind`, Docker, Kind, kubectl, Helm, and `openssl`. Build and load images from the root [README](../../../README.md), create the external `web-crawler-postgres` Secret in the namespace, then install or upgrade:

```sh
helm --kube-context kind-kind upgrade --install web-crawler deploy/helm/web-crawler \
  --namespace web-crawler --create-namespace --wait --timeout 8m
```

## Resource budgets and persistence

Every pod container has CPU and memory requests and limits, including the PostgreSQL wait init container in API and worker pods. Exact values and per-container scope are in the [Kubernetes resource budget](../../../docs/kubernetes-resources.md) and [chart values](values.yaml). PostgreSQL's 10Gi PVC uses `Retain` when the StatefulSet is deleted or scaled down. Removing the Helm release preserves the PVC; deleting the namespace or claim removes the local archive. No backup/restore path is included.

## Undeploy and use

```sh
helm --kube-context kind-kind uninstall web-crawler --namespace web-crawler
```

Keep `web-crawler-postgres` and the PVC for retained local data. Port-forward only the frontend to `127.0.0.1:8080`. See the [System Design](../../../docs/system-design.md), [contracts](../../../docs/contracts/README.md), and [documentation index](../../../docs/README.md).


## AI development disclaimer

> **AI development disclaimer:** This project was built entirely with GPT-6 Luna at Max effort as a proof of concept exploring how low-cost AI plans can be useful when paired with disciplined harness and loop engineering. This is project-owner attribution; repository contents do not independently verify runtime model metadata. Review AI-generated design and code before relying on them.
