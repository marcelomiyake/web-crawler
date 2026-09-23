# ADR-0004: Run the Web Crawler on Kubernetes

- **Status:** Implemented (retrospective)
- **Recorded:** 2026-09-25
- **Original decision date:** Unknown from repository evidence
- **Decision owner:** Project owner
- **Confirmation:** Current implementation is documented at the project owner’s request; historical team approval is not recorded.

> This record documents the technology in the current implementation. The options and rationale below are a retrospective comparison, not a claim that the original project formally evaluated them.

## Contents

- [Context and problem statement](#context-and-problem-statement)
- [Decision drivers](#decision-drivers)
- [Options considered](#options-considered)
- [Decision outcome](#decision-outcome)
- [Consequences](#consequences)
- [Evidence and realization](#evidence-and-realization)
- [Review triggers](#review-triggers)
- [References](#references)

## Context and problem statement

The chart deploys separate application Deployments and a persistent PostgreSQL StatefulSet. The local services are ClusterIP-only and the UI uses a loopback-bound port-forward. The cluster is local and single-node, with no production availability claim.

The scope of this decision is Kubernetes on local Kind for the Vue frontend, Rust API, Rust worker, and PostgreSQL. The source confirms the implementation; its historical selection rationale and original option set are not recorded.

## Decision drivers

- Deploy API, worker, and frontend independently with separate replica counts.
- Keep PostgreSQL persistent and reachable only within the cluster.
- Declare pod CPU and memory requests and limits, including init containers.

## Options considered

### Kubernetes on local Kind

- **Benefits:** Matches the checked-in chart, internal Services, and local deployment runbook.
- **Costs and risks:** Requires a cluster, persistent storage, and host resource budget; one node is a single failure domain.

### Docker Compose

- **Benefits:** Could reduce local orchestration setup.
- **Costs and risks:** Would not exercise Kubernetes resource, Service, StatefulSet, and claim behavior.

### Direct host processes

- **Benefits:** Could shorten setup for code-only tasks.
- **Costs and risks:** Would not reproduce the current multi-service networking and storage configuration.

## Decision outcome

Retain Kubernetes resources and Kind for the documented local profile. Keep the UI loopback-only and avoid inferring production safety from local deployment results.

## Consequences

### Positive

- The API, worker, frontend, and database have explicit deployment boundaries.
- Resource requests and limits are reviewable with the deployment manifests.

### Negative and risks

- Local Kind shares host resources; the database remains a single local instance.
- Kubernetes setup and storage behavior increase local operational complexity.

## Evidence and realization

- [templates](../../deploy/helm/web-crawler/templates)
- [kubernetes-resources.md](../kubernetes-resources.md)
- [system-design.md](../system-design.md)
- [README.md](../../README.md)

## Review triggers

- Reconsider if the deployment target or operational constraints change.
- Before any shared deployment, address authentication, egress controls, backups, storage, and capacity.

## References

- [web-crawler README](../../README.md)
- [System Design](../system-design.md)
- [ADR practices](https://adr.github.io/ad-practices/)
- [ADR template guidance](https://adr.github.io/adr-templates/)
