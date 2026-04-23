# Replay fixture layout contract

This directory is reserved for normalized replay fixtures that back parity tests.

Expected layout:
- `http/<family>/<name>.request.json`
- `http/<family>/<name>.response.json`
- `ws/<family>/<name>.event.json`
- `workflow/<family>/<name>.json`

Rules:
- Fixtures must be scrubbed of secrets and user-identifying data.
- Private-interface fixtures should include a stability note in metadata.
- A fixture should map back to at least one catalog entry in `coverage-matrix.json`.
