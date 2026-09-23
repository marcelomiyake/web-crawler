#!/usr/bin/env python3
"""Rewrite LLVM coverage source paths relative to the Sonar project root."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REPORT = ROOT / "target" / "sonar-rust-lcov.info"

if not REPORT.is_file():
    raise SystemExit(f"Rust LCOV report is missing: {REPORT}")

normalized = []
for line in REPORT.read_text(encoding="utf-8").splitlines():
    if line.startswith("SF:"):
        source = Path(line[3:])
        if source.is_absolute():
            try:
                source = source.resolve().relative_to(ROOT)
            except ValueError as error:
                raise SystemExit(f"LCOV source is outside the repository: {source}") from error
        line = f"SF:{source.as_posix()}"
    normalized.append(line)

REPORT.write_text("\n".join(normalized) + "\n", encoding="utf-8")

records = []
record = []
for line in normalized:
    record.append(line)
    if line == "end_of_record":
        records.append(record)
        record = []
if record:
    records.append(record)

scopes = {
    "api": ("crawler-api/src/", "crates/crawler-config/src/", "crates/crawler-domain/src/"),
    "worker": ("crawler-worker/src/", "crates/crawler-config/src/", "crates/crawler-domain/src/"),
}
for service, prefixes in scopes.items():
    scoped = []
    for item in records:
        source = next((line[3:] for line in item if line.startswith("SF:")), "")
        if source.startswith(prefixes):
            scoped.extend(item)
    output = ROOT / "target" / f"sonar-{service}-rust-lcov.info"
    output.write_text("\n".join(scoped) + "\n", encoding="utf-8")
