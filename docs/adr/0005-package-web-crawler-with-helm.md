# ADR-0005: Package the Web Crawler with Helm

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

The repository chart packages frontend, API, worker, and PostgreSQL resources. Values and templates specify replicas, resource budgets, services, and persistent storage. The runbook separates uninstalling the release from deleting the namespace and its database claim.

The scope of this decision is Helm chart and release lifecycle for the local Kubernetes deployment. The source confirms the implementation; its historical selection rationale and original option set are not recorded.

## Decision drivers

- Keep local deployment resources and values reviewable in one chart.
- Make upgrade, rollback, and removal procedures repeatable.
- Preserve PostgreSQL data during ordinary app uninstall and explain destructive cleanup.

## Options considered

### Helm

- **Benefits:** Matches the existing chart and named release commands.
- **Costs and risks:** Template rendering and claim retention need deliberate review.

### Kustomize or raw manifests

- **Benefits:** Could reduce templating and present concrete YAML.
- **Costs and risks:** Would replace existing scripts and release workflow.

### Docker Compose

- **Benefits:** Could be a simpler local container option.
- **Costs and risks:** Would diverge from the existing Kubernetes deployment configuration.

## Decision outcome

Retain Helm as the Kubernetes packaging boundary. Use the documented uninstall operation for application removal and treat namespace/PVC deletion as data destruction.

## Consequences

### Positive

- Chart values centralize local replica and resource configuration.
- The release history supports repeatable local update and rollback operations.

### Negative and risks

- Rendered YAML must be reviewed because template correctness is not implied by valid syntax.
- PVC retention does not replace backup and restore.

## Evidence and realization

- [Chart.yaml](../../deploy/helm/web-crawler/Chart.yaml)
- [values.yaml](../../deploy/helm/web-crawler/values.yaml)
- [templates](../../deploy/helm/web-crawler/templates)
- [README.md](../../README.md)

## Review triggers

- Reconsider if database lifecycle needs independent ownership or the platform changes.
- Update this ADR when chart storage behavior or deployment commands change.

## References

- [web-crawler README](../../README.md)
- [System Design](../system-design.md)
- [ADR practices](https://adr.github.io/ad-practices/)
- [ADR template guidance](https://adr.github.io/adr-templates/)
