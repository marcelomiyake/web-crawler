# SonarQube Cloud verification

`crawler-api` and `crawler-worker` are separate SonarQube Cloud projects. The [GitHub Actions workflow](https://github.com/marcelomiyake/web-crawler/actions/workflows/sonarcloud-microservices.yml) runs one coverage and analysis job per service on pushes to `main`; `workflow_dispatch` supports a manual rerun. Organization-level auto-import of new GitHub repositories is disabled, so this workflow owns analysis.

Each job provisions PostgreSQL, runs `cargo llvm-cov --lcov --output-path target/coverage/lcov.info -- --test-threads=1` from the service directory, and imports that report with `cargo sonar-scanner`. It reads the repository secret named `SONAR_TOKEN`. Neither project excludes `src/main.rs`; the percentages include startup and application wiring. No local Sonar scan script is used.

## Projects and coverage

Coverage below is SonarCloud's overall line coverage for `main`, not new-code or local coverage. The baseline is the latest Cloud result before the coverage-test updates; the current column is the latest result after them. Values are from the 2026-09-27 snapshot.

| Microservice | SonarCloud project | Before | Current | Change |
| --- | --- | ---: | ---: | ---: |
| `crawler-api` | [project](https://sonarcloud.io/project/overview?id=marcelomiyake_web-crawler_crawler-api) | 94.6% | 95.4% | +0.8 pp |
| `crawler-worker` | [project](https://sonarcloud.io/project/overview?id=marcelomiyake_web-crawler_crawler-worker) | 80.4% | 81.1% | +0.7 pp |

Both projects have passing Quality Gates, zero open or confirmed issues, zero hotspots awaiting review, zero bugs, zero vulnerabilities, zero code smells, and 0.0% duplicated lines.

## Verification

Confirm the latest workflow completed for the pushed commit and each project's `main` analysis matches that revision. Review coverage, active issues, security hotspots, duplication, and the Quality Gate. Project links above open the live dashboards; the workflow link shows the run history. Local test results do not replace a completed Cloud analysis.
