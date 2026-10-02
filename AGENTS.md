# Repository engineering instructions

## Scope and structure

This is a Rust 2024 stdio MCP filesystem server for macOS and Linux, with a static local observer dashboard. Linux requires `/proc/self/fd`. Run repository commands from this directory and keep changes within the requested scope.

- `src/main.rs` and `src/protocol.rs`: CLI, stdio lifecycle, and JSON-RPC contracts.
- `src/tools/`: tool schemas, input validation, and dispatch.
- `src/workspace/` and `src/security/`: handle-relative filesystem operations, path policy, and rooted aliases.
- `src/unified_patch.rs`: confined unified text-diff parsing and preflight.
- `src/tunnel.rs` and `src/tunnel/`: dotenv configuration and tunnel-client launch.
- `src/observer/` and `src/observer_config.rs`: metadata metrics, HTTP listener, SQLite storage, and volume configuration.
- `observer-ui/`: plain HTML, CSS, and browser JavaScript embedded into the binary.
- `tests/`: headless stdio, workspace, observer, and fake-tunnel integration coverage.

Reuse existing helpers and dependencies. There is no frontend package manager or asset build pipeline. SQL and UI assets use `include_str!` / `include_bytes!`; rebuild the binary after changing them.

## Behavioral invariants

Keep stdout exclusively for line-delimited JSON-RPC responses. Diagnostics belong on stderr. Do not add shell execution tools or weaken workspace path, ignore, denylist, symlink, or root-mutation checks. Filesystem mutations require an explicit root.

Preserve supported tool arguments, result shapes, CLI/env precedence, error behavior, and partial-operation reporting unless the request explicitly changes them. Atomicity is per file, not across a multi-file patch. Expected hashes remain optimistic conflict checks.

Observer telemetry must remain metadata-only. Never persist or export arguments, result content, error messages, arbitrary tool names, filenames, workspace paths, raw request IDs, environment values, or credentials. Keep the observer on loopback with its existing HTTP boundaries and the database outside the workspace.

Append numbered SQL files under `src/observer/storage/migrations/` and register the next sequential version in `migrations.rs`. Keep applied migrations immutable. Migration SQL and version updates must remain transactional, idempotent on reopening, and covered for rollback and newer-schema rejection.

Do not print or copy real `.env` values into commands, reviews, fixtures, or diagnostics. Use synthetic credentials and isolated temporary workspaces/databases for development and automated tests. A user-authorized secret audit may inspect local values without disclosing them. Do not contact a real tunnel or modify a user's active server for automated tests. Keep local configuration, SQLite volumes and companions, exports, and build artifacts out of Git.

## Workflow and checks

For behavior, API, schema, or data-contract changes, use **Red → Green → Refactor → Prove**: run a focused failing test first, implement the smallest fix, then run relevant broader checks. Use existing passing behavioral coverage for a behavior-preserving replacement; documentation and mechanical changes do not require artificial failures. Do not weaken tests to obtain green.

Canonical checks:

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
node --check observer-ui/app.js
node --check observer-ui/charts.js
```

Focused observer checks:

```bash
cargo test --locked --bin file-system-mcp observer::
cargo test --locked --test observer_integration --test tunnel_launcher
```

For dashboard changes, verify keyboard access, narrow-screen layout, storage and connection failures, refresh, filtering, and export using disposable data. Keep new or materially changed handwritten files under 500 lines and functions focused; split by responsibility rather than by arbitrary metrics.

Update affected public docs when behavior changes. In the handoff, report changed behavior, the failing regression or characterization baseline, exact commands and outcomes, and material limits. Do not claim an unrun check passed. Review changes for secrets before publishing; publication, credential use, and changes to live services need the user's explicit request.
