# SonarQube verification

## Scope

- **Server:** SonarQube Community Build 26.9.0.129388 at `http://127.0.0.1:9000`.
- **Analysis date:** 2026-09-25 (America/Sao_Paulo).
- **Source revision:** `948fc8c0de2023396ad97f6deafdbcb1e6fcf2a5` plus the current uncommitted worktree; no commit was created.
- **Command:** `SONAR_HOST_URL=http://127.0.0.1:9000 SONAR_TOKEN=... ./.sonar/scan.sh`.
- The scanner used the existing Rust Clippy/LCOV and frontend LCOV reports; this scan command did not regenerate them or rerun application tests.

> Project documentation index: [Documentation index](../README.md)

## Results

| Sonar project | Coverage | Line coverage | Branch coverage | Duplication | Open issues | Bugs / vulnerabilities / code smells / hotspots | Quality gate |
|---|---:|---:|---:|---:|---:|---:|---|
| [`web-crawler-api`](http://127.0.0.1:9000/dashboard?id=web-crawler-api) | 95.4% | 95.4% (883/926) | Not reported | 0.0% | 0 | 0 / 0 / 0 / 0 | **Passed** |
| [`web-crawler-worker`](http://127.0.0.1:9000/dashboard?id=web-crawler-worker) | 81.8% | 81.8% (1377/1683) | Not reported | 0.0% | 0 | 0 / 0 / 0 / 0 | **Passed** |
| [`web-crawler-frontend`](http://127.0.0.1:9000/dashboard?id=web-crawler-frontend) | 85.4% | 91.5% (161/176) | 78.6% (125/159) | 0.0% | 0 | 0 / 0 / 0 / 0 | **Passed** |

All three server quality gates passed. SonarQube's aggregate coverage measure combines line and condition coverage; the frontend branch measure is below 80% even though its server gate passes.

## Limits

The scan used the reports already present in the worktree, so the reported coverage is only as current as those reports. The configured Sonar sources cover Rust and frontend application code; repository Markdown, Helm chart files, `index.html`, and public static assets are outside those source paths and are not analyzed by this run. Lighthouse and SEO metadata results are in [the browser verification record](lighthouse.md).
