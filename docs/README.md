# Documentation index

Use this index to find the repository overview, agent guidance, architecture, contracts, operational instructions, verification records, and reusable templates. This static Markdown index has no application build, deployment, or undeployment lifecycle.

## Contents

- [Document responsibilities](#document-responsibilities)
- [Project and component guides](#project-and-component-guides)
- [Agent instructions](#agent-instructions)
- [Architecture and contracts](#architecture-and-contracts)
- [Build, deployment, and operations](#build-deployment-and-operations)
- [Verification and evaluation](#verification-and-evaluation)
- [Design notes and decision records](#design-notes-and-decision-records)
- [Standards and reusable templates](#standards-and-reusable-templates)
- [Applying the standard](#applying-the-standard)
- [AI development disclaimer](#ai-development-disclaimer)

## Document responsibilities

- This index ([docs/README.md](README.md)) links every project Markdown document by purpose.
- Root and component `README.md` files help people build, deploy, use, and contribute.
- `AGENTS.md` files give scoped instructions to coding agents.
- Architecture and contract records identify system boundaries, owners, producers, consumers, and authoritative sources.
- Operational and verification documents explain applicable lifecycle commands and recorded evidence.

## Project and component guides

- [README.md](../README.md) — Root or component README
- [crawler-api/README.md](../crawler-api/README.md) — Root or component README
- [crawler-frontend/README.md](../crawler-frontend/README.md) — Root or component README
- [crawler-worker/README.md](../crawler-worker/README.md) — Root or component README
- [docs/README.md](README.md) — Documentation index

## Agent instructions

- [docs/agent-documentation-hooks.md](agent-documentation-hooks.md) — Codex hook recommendation for post-agent documentation sync

- [AGENTS.md](../AGENTS.md) — Scoped agent guidance
- [crawler-api/AGENTS.md](../crawler-api/AGENTS.md) — Scoped agent guidance
- [crawler-frontend/AGENTS.md](../crawler-frontend/AGENTS.md) — Scoped agent guidance
- [crawler-worker/AGENTS.md](../crawler-worker/AGENTS.md) — Scoped agent guidance


- [CLAUDE.md](../CLAUDE.md) — Single-line import of scoped `AGENTS.md` guidance
- [crawler-api/CLAUDE.md](../crawler-api/CLAUDE.md) — Single-line import of scoped `AGENTS.md` guidance
- [crawler-frontend/CLAUDE.md](../crawler-frontend/CLAUDE.md) — Single-line import of scoped `AGENTS.md` guidance
- [crawler-worker/CLAUDE.md](../crawler-worker/CLAUDE.md) — Single-line import of scoped `AGENTS.md` guidance

## Architecture and contracts

- [docs/contracts/README.md](contracts/README.md) — API/data contract catalog
- [docs/database-model.md](database-model.md) — database tables, columns, ownership, and descriptions
- [docs/system-design.md](system-design.md) — System Design or architecture decision

- [docs/adr/README.md](adr/README.md) — Architectural Decision Records index and maintenance guidance
- [docs/adr/0001-use-rust-for-backend-services.md](adr/0001-use-rust-for-backend-services.md) — ADR-0001: Use Rust for Backend Services
- [docs/adr/0002-use-vue-for-crawler-frontend.md](adr/0002-use-vue-for-crawler-frontend.md) — ADR-0002: Use Vue for the Crawler Frontend
- [docs/adr/0003-use-postgresql-for-crawl-state.md](adr/0003-use-postgresql-for-crawl-state.md) — ADR-0003: Use PostgreSQL for Crawl State
- [docs/adr/0004-run-web-crawler-on-kubernetes.md](adr/0004-run-web-crawler-on-kubernetes.md) — ADR-0004: Run the Web Crawler on Kubernetes
- [docs/adr/0005-package-web-crawler-with-helm.md](adr/0005-package-web-crawler-with-helm.md) — ADR-0005: Package the Web Crawler with Helm

## Build, deployment, and operations

- [deploy/helm/web-crawler/README.md](../deploy/helm/web-crawler/README.md) — Helm chart deployment guide
- [docs/kubernetes-resources.md](kubernetes-resources.md) — Kubernetes resource budget and good practice

## Verification and evaluation

- [docs/verification/README.md](verification/README.md) — Verification evidence index
- [docs/verification/lighthouse.md](verification/lighthouse.md) — Lighthouse and SEO metadata check
- [docs/verification/sonarqube.md](verification/sonarqube.md) — API, worker, and frontend SonarQube results
- [docs/system-design.md](system-design.md#9-verification-and-acceptance) — System-level unit, integration, Kind, SonarQube, and JEV notes

## Design notes and decision records

- [crawler-frontend/DESIGN.md](../crawler-frontend/DESIGN.md) — Design notes or decision record

## Standards and reusable templates

- [docs/documentation-standard.md](documentation-standard.md) — Documentation standard
- [docs/templates/adr.template.md](templates/adr.template.md) — Reusable Markdown ADR template
- [docs/templates/agents.template.md](templates/agents.template.md) — Reusable Markdown template
- [docs/templates/api-contract.template.md](templates/api-contract.template.md) — Reusable Markdown template
- [docs/templates/component-readme.template.md](templates/component-readme.template.md) — Reusable Markdown template
- [docs/templates/design-notes.template.md](templates/design-notes.template.md) — Reusable Markdown template
- [docs/templates/design-report.template.md](templates/design-report.template.md) — Reusable Markdown template
- [docs/templates/general-document.template.md](templates/general-document.template.md) — Reusable Markdown template
- [docs/templates/operations-guide.template.md](templates/operations-guide.template.md) — Reusable Markdown template
- [docs/templates/root-readme.template.md](templates/root-readme.template.md) — Reusable Markdown template
- [docs/templates/system-design.template.md](templates/system-design.template.md) — Reusable Markdown template
- [docs/templates/verification-record.template.md](templates/verification-record.template.md) — Reusable Markdown template

## Applying the standard

- [Documentation standard](documentation-standard.md)
- [Component README template](templates/component-readme.template.md)


## AI development disclaimer

> **AI development disclaimer:** This project was built entirely with GPT-6 Luna at Max effort as a proof of concept exploring how low-cost AI plans can be useful when paired with disciplined harness and loop engineering. This is project-owner attribution; repository contents do not independently verify runtime model metadata. Review AI-generated design and code before relying on them.
