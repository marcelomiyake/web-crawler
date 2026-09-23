# ADR-0001: Use Rust for Backend Services

- **Status:** Implemented (retrospective)
- **Recorded:** 2026-09-25
- **Original decision date:** Unknown from repository evidence
- **Decision owner:** Project owner
- **Confirmation:** Current implementation is documented at the project owner's request; historical team approval is not recorded.

> This record captures the Rust API and worker already present in the repository. The options and rationale below are a retrospective comparison, not a claim that the original project formally evaluated them.

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

The Web Crawler uses a Rust/Axum API for crawl-job lifecycle operations and a Rust/Tokio worker for durable frontier claims, robots policy, safe outbound fetches, HTML extraction, and page persistence. PostgreSQL stores job, frontier, lease, and archive state; a Vue application provides the local user interface.

The project owner's rationale is that Rust is safer, fast, and can use less CPU and memory. The owner also recognizes Rust's deeper learning curve and considers it feasible with AI-assisted code authoring. These are design expectations and the owner's experience, not results from a comparative benchmark in this repository.

## Decision drivers

- Use compile-time ownership and type checks in concurrent services that perform network access and durable state transitions.
- Keep runtime overhead and expected CPU/memory needs suitable for configured local Kubernetes budgets.
- Express URL, robots, lease, retry, and persistence invariants explicitly.
- Make Rust development feasible for this proof of concept through AI assistance, compiler feedback, focused tests, and human review.

## Options considered

### Rust for the API and worker

- **Benefits:** Compiled native services, strong ownership and type checks, and low runtime overhead suit concurrent network services.
- **Costs and risks:** Ownership, lifetimes, async types, and compiler diagnostics create a steeper learning curve; builds may take longer than a small scripting service.

### Go for the API and worker

- **Benefits:** Static typing, approachable concurrency primitives, and a straightforward service deployment model.
- **Costs and risks:** Switching would add churn without evidence that Rust is a bottleneck; garbage collection and its type/concurrency model have different trade-offs.

### TypeScript/Node.js for the API and worker

- **Benefits:** Could reuse the frontend language and simplify language switching.
- **Costs and risks:** Managed runtime behavior and lack of Rust ownership checks change the safety and resource profile; resource claims would need representative measurement.

No cross-language performance benchmark is available, so the comparison is qualitative.

## Decision outcome

Use Rust for the crawler API and worker. AI-assisted authoring makes the learning curve acceptable for this proof of concept when paired with compiler checks, automated tests, documented contracts, and human review. Generated code is not treated as verified solely because it compiles or was produced by a capable model.

## Consequences

### Positive

- The ownership and type system catch many memory-safety and concurrency errors before deployment.
- Native binaries and low runtime overhead are expected to fit configured local budgets; profiling under representative crawl load is required to verify actual consumption.
- AI assistance can help contributors navigate Rust while compiler feedback, tests, and review preserve accountability.

### Negative and risks

- Contributors need time to learn Rust ownership, async networking, and the crawler's domain rules.
- Local builds and CI require Rust tooling and locked dependencies.
- Reviewers must check URL/SSRF protections, robots handling, lease behavior, and persistence invariants; generated code can still introduce unsafe behavior or incorrect policy.

## Evidence and realization

- The [API manifest](../../crawler-api/Cargo.toml), [worker manifest](../../crawler-worker/Cargo.toml), and workspace manifest define Rust services and shared crates.
- The [System Design](../system-design.md) describes service ownership, PostgreSQL coordination, network safeguards, and deployment shape.
- The [Kubernetes resource budget](../kubernetes-resources.md) lists configured CPU and memory requests and limits; these are not comparative benchmark results.

## Review triggers

- Reconsider if representative profiling shows the Rust services miss measured performance or resource targets and a controlled alternative-language comparison indicates a better fit.
- Reconsider if Rust's learning or maintenance cost remains a material delivery risk despite AI assistance, compiler tooling, tests, and review.
- Create a new ADR before migrating a backend component to another language or runtime.

## References

- [Web Crawler README](../../README.md)
- [System Design](../system-design.md)
- [ADR practices](https://adr.github.io/ad-practices/)
- [ADR template guidance](https://adr.github.io/adr-templates/)
