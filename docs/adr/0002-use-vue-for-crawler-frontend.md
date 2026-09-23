# ADR-0002: Use Vue for the Crawler Frontend

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

The browser client creates bounded crawl jobs, displays progress, and reads archived pages through the Rust API. The UI is served locally and intended for loopback port-forwarding, not public access. The original framework selection record was not found.

The scope of this decision is Vue 3 and TypeScript for the local crawl management and archive UI. The source confirms the implementation; its historical selection rationale and original option set are not recorded.

## Decision drivers

- Support interactive job submission, status refresh, and archive browsing.
- Keep UI state and rendering separate from crawler policy and outbound networking.
- Retain browser-level tests for API flows and accessible controls.

## Options considered

### Vue 3 with TypeScript

- **Benefits:** The current component app and Vite toolchain implement the local workflow.
- **Costs and risks:** Adds framework and Node.js maintenance alongside the Rust services.

### React with TypeScript

- **Benefits:** Could implement the same application model and testing approach.
- **Costs and risks:** Would require replacement of existing components without evidence that Vue fails the use case.

### Server-rendered pages

- **Benefits:** Could reduce client-side code for a local administration tool.
- **Costs and risks:** Would require redesigning current status refresh and interactive archive flows.

## Decision outcome

Retain Vue 3 and TypeScript for the local UI. Keep crawl policy and network safety checks in backend services.

## Consequences

### Positive

- The browser remains a presentation client and does not perform crawl requests.
- UI verification can exercise progress and archive flows independently of worker internals.

### Negative and risks

- The UI has no authentication and must remain within the documented trusted local environment.
- The repository does not include a framework comparison benchmark.

## Evidence and realization

- [package.json](../../crawler-frontend/package.json)
- [Dockerfile](../../crawler-frontend/Dockerfile)
- [system-design.md](../system-design.md)
- [README.md](../../README.md)

## Review triggers

- Reconsider if the supported client platform or accessibility requirements change.
- Create a new ADR before moving crawl logic into the browser or replacing Vue.

## References

- [web-crawler README](../../README.md)
- [System Design](../system-design.md)
- [ADR practices](https://adr.github.io/ad-practices/)
- [ADR template guidance](https://adr.github.io/adr-templates/)
