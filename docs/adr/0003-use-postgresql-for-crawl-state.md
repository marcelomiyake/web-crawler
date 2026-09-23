# ADR-0003: Use PostgreSQL for Crawl State

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

The API and worker share PostgreSQL for v1. Transactions, uniqueness constraints, row locks, and leases coordinate frontier work and host politeness. No broker or Redis service is deployed. The original database selection record was not found.

The scope of this decision is PostgreSQL as the durable store for crawl jobs, frontier URLs, host coordination, and archived page metadata. The source confirms the implementation; its historical selection rationale and original option set are not recorded.

## Decision drivers

- Commit related crawl job, URL, lease, and archive changes with explicit transaction semantics.
- Coordinate multiple worker replicas and deduplicate discovered URLs.
- Keep the durable frontier and outcomes recoverable after process restarts.

## Options considered

### PostgreSQL

- **Benefits:** Relational transactions, constraints, and row locking support current durable job and frontier behavior.
- **Costs and risks:** The local deployment is a single-replica database and is not highly available.

### Redis queue plus relational metadata

- **Benefits:** Could provide an in-memory fast coordination path.
- **Costs and risks:** Would add a second consistency and recovery model for state that currently commits with PostgreSQL.

### Cassandra or another distributed key-value store

- **Benefits:** Could fit a measured high-volume partitioned frontier.
- **Costs and risks:** Would require explicit designs for leases, quotas, transactional admission, archive deletion, and recovery.

## Decision outcome

Retain PostgreSQL as the durable coordination and archive store for v1. Do not add Redis or a broker until a measured need and atomicity/recovery design justify it.

## Consequences

### Positive

- Frontier claims, host leases, and job state can share transaction and constraint boundaries.
- The durable database is the recovery source after API or worker restarts.

### Negative and risks

- A single local PostgreSQL instance is a failure domain and capacity limit.
- API and worker share schema/migrations, so their data coupling remains explicit.

## Evidence and realization

- [0001_initial.sql](../../database/postgres/migrations/0001_initial.sql)
- [Cargo.toml](../../crawler-api/Cargo.toml)
- [Cargo.toml](../../crawler-worker/Cargo.toml)
- [values.yaml](../../deploy/helm/web-crawler/values.yaml)
- [database-model.md](../database-model.md)
- [system-design.md](../system-design.md)

## Review triggers

- Reassess after measured frontier throughput, storage growth, or lock contention shows PostgreSQL no longer meets the local or intended workload.
- Any queue or cache addition must document transaction handoff, recovery, and source-of-truth behavior.

## References

- [web-crawler README](../../README.md)
- [System Design](../system-design.md)
- [ADR practices](https://adr.github.io/ad-practices/)
- [ADR template guidance](https://adr.github.io/adr-templates/)
