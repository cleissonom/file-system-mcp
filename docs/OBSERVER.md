# MCP Observer

The black, grey, and red dashboard observes actual `tools/call` requests inside one MCP process and its database. It shows total calls, success rate, mean and recent p95 latency, all in-flight requests, request/response bytes, per-tool performance, source labels and attribution evidence, anonymous server runs, a minute chart, and a filterable call stream. Concurrent processes need separate databases and ports; this is not a central aggregator. Snapshots refresh every two seconds and can be paused or triggered manually. Uptime and active elapsed times tick locally every second between snapshots, freezing when paused or disconnected until a fresh snapshot succeeds. JSON export downloads the current bounded snapshot, not the complete database archive.

The observer does not see model token usage or cost. Telemetry contains a generated sequence, known tool name, outcome, timestamps, duration, byte counts, fixed typed origin fields, and an anonymous server-run ID. Arguments, returned content, error messages, filenames, workspace paths, raw request IDs, raw client/identity metadata, raw environment values, and credentials are never retained or exported. Unrecognized tool names share an `unknown_tool` bucket.

## Starting the dashboard

`file-system-mcp tunnel` enables the observer by default at **http://127.0.0.1:9411**. Use `tunnel --no-dashboard` to disable it, or `tunnel --dashboard-port <PORT>` / `MCP_DASHBOARD_PORT` to choose a port. The CLI takes precedence.

Direct stdio startup is opt-in:

```bash
./target/release/file-system-mcp --root "$HOME/Projects/workspace" --dashboard --dashboard-port 9411
```

Port `0` selects a free port and prints its URL to stderr. An occupied port fails startup with an error. Normal direct startup without `--dashboard` does not create telemetry storage. Keep stdin open through a stdio MCP client; the dashboard shuts down with the MCP process.

The listener binds to IPv4 loopback and serves fixed assets plus read-only `GET`/`HEAD` endpoints at `/api/snapshot` and `/api/export`. It requires its own localhost Host/Origin, does not enable CORS, and bounds request headers and HTTP read/write time. The observer does not alter stdout protocol responses. It has no authentication and is intended for local use; do not expose it publicly.

## History and metric definitions

Completed calls are saved to SQLite and restored after restart. Totals and per-tool aggregates cover saved history plus calls observed by the current process. Recent calls and p95 cover at most the latest 1,000 completed calls; the chart covers the last 60 minutes using completion time.

Total calls include currently started calls. Outcome counts and latency cover completed calls. PID, uptime, and active calls describe the current process. Calls interrupted before completion are not stored. Metrics from an older memory-only process cannot be recovered after it exits.

The dedicated in-flight table displays every request currently executing: tool, catalog capability, source/evidence, server run, monotonic elapsed time, and request bytes. Short calls can finish between two-second snapshots and only appear in completed history. Requests queued outside this MCP process are not visible. `read` and `write` describe whether a tool is read-only or write-capable; a write-capable call does not prove that files changed.

Security counters summarize the unfiltered retained recent window of at most 1,000 completed calls: write-capable calls, failed calls, and unknown origins. They are not lifetime counts or a record of actual mutations. Origin totals cover all saved history plus calls observed by the current process.

Storage status reports the backend kind, schema version, health of the latest append, and a cumulative write-failure count for the current process. A later successful append restores the health indicator but does not erase the failure count. The UI keeps storage loss warnings separate from dashboard connection errors.

## Source labels and server runs

Supported source values are `unknown`, `chatgpt`, `chatgpt_work`, `codex`, `codex_cloud`, and `openai_dot`. Configure a dedicated connection with `--observer-source <SOURCE>` or `MCP_OBSERVER_SOURCE`; the default is `unknown`. Direct stdio startup uses CLI over shell and does not read `.env`. Tunnel startup uses CLI over shell over `.env`. The direct flag requires `--dashboard`; `tunnel --observer-source` conflicts with `--no-dashboard`.

A client or adapter you control can supply a per-call custom extension in the `tools/call` parameters:

```json
{
  "name": "workspace_info",
  "arguments": {},
  "_meta": {"file-system-mcp/source": "codex_cloud"}
}
```

A recognized non-unknown tag is marked `client_reported` and remains unverified. When the tag is absent, a configured non-unknown label is marked `operator_configured`. A present malformed or unsupported tag becomes `unknown`, even when the connection has a configured label; unavailable attribution is also `unknown`. ChatGPT does not automatically supply this extension. Source labels are not authenticated caller identity or authorization. Transport (`stdio` or `tunnel`) describes launch context only.

The observer does not infer origins from `clientInfo`, user-agent, session, or identity hints. The latest `initialize` message would not reliably attribute calls multiplexed through one stdio connection. Only the fixed fields above are retained, never arbitrary client metadata.

The API field `session_id`, displayed as **Run #**, identifies an anonymous server process/run within this database, not a conversation, user, or authenticated session. A new run uses the highest saved run ID plus one. A run that saves no completed call leaves no run record and can reuse that ID at the next startup. Older rows retain unknown origin and run `0`, displayed as unavailable rather than guessed attribution.

## SQLite storage and migrations

The database is created automatically when the observer starts. Its default is `.data/observer.sqlite3` beside the executable's nearest ancestor `Cargo.toml`, independent of the working directory. A relocated standalone binary uses `.data` beside itself. SQLite is bundled into the server; no database service or SQLite CLI installation is needed.

Override the path with `MCP_OBSERVER_DB` or `--dashboard-db`. For tunnel startup, the setting may be in `.env` or the shell. CLI values take precedence over shell, then `.env`. Relative file/shell paths in tunnel mode resolve against the `.env` directory; relative CLI paths resolve against the launch directory. Direct stdio startup reads the flag/environment but does not load `.env`, and relative paths use the current directory:

```bash
./target/release/file-system-mcp --root "$HOME/Projects/workspace" --dashboard --dashboard-db /path/to/private/observer.sqlite3
```

Keep the database **outside the workspace** so tools cannot read, move, or delete their own telemetry volume. Startup rejects paths inside the root, including canonical parent aliases, before creating the volume. New database files use owner-only permissions. Unsafe file/directory ownership or permissions are rejected. An adjacent `<database>.lock` file enforces one observer per database. Database and lock-file symlinks or hard links are rejected. Do not delete an active instance's lock file; use separate databases and ports for concurrent servers.

Embedded migration SQL lives in [`src/observer/storage/migrations/`](../src/observer/storage/migrations/). Startup reads SQLite's application-owned [`PRAGMA user_version`](https://www.sqlite.org/pragma.html#pragma_user_version), applies pending sequential migrations, and records each successful version in the same transaction. It also validates the application's database identity, rejecting nonempty databases owned by another application. Reopening an up-to-date database preserves data. A failed migration rolls back that step; previously completed steps remain committed. A newer schema is rejected rather than downgraded.

Migration `0003_origin.sql` runs automatically to add fixed source, attribution evidence, transport, and anonymous run fields. Existing rows are preserved with unknown origin and run `0`; historical identities are not inferred.

To add a migration, create the next numbered SQL file and register its sequential version in [`migrations.rs`](../src/observer/storage/migrations.rs). Keep previously applied migrations unchanged. Rebuild the server so new SQL is embedded, and add tests for upgrade, reopening, and rollback.

Each completed call is written synchronously using WAL, full synchronization, and a bounded busy timeout. A runtime write failure leaves the MCP result unchanged and live metrics available, while the dashboard warns that calls could not be saved. Failed records are not replayed and are absent after restart. Startup storage errors fail visibly instead of falling back silently to memory.

The database retains all completed metadata and grows with use. The 1,000-call and 60-minute limits apply to dashboard views and in-memory caches; they do not prune disk history. Back up with all database users stopped or use SQLite backup tooling. Do not copy only the main database while WAL writes are active.

The local `.data/` volume, databases, WAL/SHM/journal companions, and lock files are ignored by Git. Migration SQL is source code and must remain checked in. Keep telemetry exports private as well: metadata can reveal activity patterns even without file contents.

## Verification

Observer unit/integration tests run with temporary volumes and fixture data:

```bash
cargo test --locked --bin file-system-mcp observer::
cargo test --locked --test observer_integration --test tunnel_launcher
node --check observer-ui/app.js
node --check observer-ui/charts.js
node --check observer-ui/activity.js
node --test tests/observer_ui.test.mjs
```

They cover metrics and retention, fixed origins and run IDs, payload exclusion, HTTP boundaries, fragmented/slow headers, occupied ports, shutdown, restart recovery, ordered migrations, failed writes, volume configuration, dashboard clocks, and composable filters. See [CONTRIBUTING.md](../CONTRIBUTING.md) for complete repository checks and manual UI verification.
