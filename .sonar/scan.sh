#!/usr/bin/env bash
set -euo pipefail

: "${SONAR_HOST_URL:?Set SONAR_HOST_URL to the SonarQube server URL.}"
: "${SONAR_TOKEN:?Set SONAR_TOKEN in the environment.}"
command -v docker >/dev/null || { printf '%s\n' 'Docker is required to run SonarScanner.' >&2; exit 1; }

WEB_CRAWLER_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
SONAR_SCANNER_IMAGE="${SONAR_SCANNER_IMAGE:-sonarsource/sonar-scanner-cli:latest}"

for report in \
  target/sonar-clippy.json \
  target/sonar-rust-lcov.info \
  crawler-frontend/coverage/lcov.info
do
  if [[ ! -s "${WEB_CRAWLER_ROOT}/${report}" ]]; then
    printf 'Required report is missing or empty: %s\n' "$report" >&2
    exit 1
  fi
done

python3 "${WEB_CRAWLER_ROOT}/.sonar/normalize-rust-lcov.py"

for report in target/sonar-api-rust-lcov.info target/sonar-worker-rust-lcov.info; do
  if [[ ! -s "${WEB_CRAWLER_ROOT}/${report}" ]]; then
    printf 'Scoped Rust coverage report is missing or empty: %s\n' "$report" >&2
    exit 1
  fi
done

for service in api worker frontend; do
  docker run --rm --network host \
    --volume "${WEB_CRAWLER_ROOT}:/usr/src" \
    --env SONAR_HOST_URL \
    --env SONAR_TOKEN \
    --workdir /usr/src \
    "$SONAR_SCANNER_IMAGE" \
    "-Dproject.settings=/usr/src/.sonar/${service}.properties" \
    "-Dsonar.working.directory=/usr/src/.sonar/working/${service}" \
    "-Dsonar.scanner.metadataFilePath=/usr/src/.sonar/working/${service}-report-task.txt" \
    -Dsonar.qualitygate.wait=true \
    -Dsonar.qualitygate.timeout=600
done
