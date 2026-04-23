# libslack parity test suite artifacts

Committed, machine-checkable parity-suite artifacts live here.

Artifacts:
- `catalog-current.md` — committed mirror of the current parity catalog snapshot
- `coverage-matrix.json` — per-family/per-interface coverage and evidence matrix
- `release-gates.json` — machine-checkable rules for seeded vs observed claims and release readiness
- `scope-policy.md` — committed non-goal / exclusion policy used as scope evidence
- `seed-observations.md` — committed summary of current seed evidence
- `live-canary-policy.md` — policy for scheduled live drift detection
- `fixtures/README.md` — normalized replay fixture layout contract
- `drift-report.example.json` — example drift-report shape for canary/replay output

These files are version-controlled, unlike `.omx/` planning notes.
