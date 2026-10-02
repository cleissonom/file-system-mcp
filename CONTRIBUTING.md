# Contributing

Contributions that improve workspace safety, MCP compatibility, or the local observer are welcome. Read the [README](README.md), [tool reference](docs/TOOLS.md), and [security policy](SECURITY.md) before changing behavior. Use [AGENTS.md](AGENTS.md) for repository-specific engineering instructions.

## Development setup

Use macOS or Linux, Rust 1.93.1 or newer as specified in `Cargo.toml`, Git for the publication-policy tests, and a C compiler for the bundled SQLite dependency. Linux needs `/proc/self/fd`. Node.js 24 is used for the dashboard JavaScript syntax checks. The dashboard uses plain browser modules; there is no npm install or frontend build step.

From the repository root:

```bash
cargo build --locked
cargo test --locked
```

The tests use temporary workspaces, a fake tunnel client, and synthetic credentials. They run headlessly without API keys, a real tunnel, or network access after Cargo dependencies have been downloaded. Do not use your personal workspace or credentials as fixtures.

To run a local stdio server, create a disposable workspace outside this repository:

```bash
mkdir -p /tmp/file-system-mcp-workspace
cargo run --locked -- --root /tmp/file-system-mcp-workspace
```

The process reads one JSON-RPC message per line from stdin. Startup messages go to stderr; stdout is reserved for protocol responses. Use a stdio MCP client to keep the connection open. Add `--dashboard --dashboard-port 0` to inspect calls through the local observer; its URL is printed to stderr. The default observer database is outside this temporary workspace, in the repository's ignored `.data/` directory.

## Making a change

Keep changes focused on a concrete problem and preserve existing contracts unless the proposed behavior change is explicit. Start a behavior or bug fix with a focused failing test, implement the smallest fix, then refactor and run the relevant checks. Documentation-only and mechanical changes do not need an artificial failing test.

Security and persistence changes need coverage of failure paths as well as success. In particular, preserve confinement under symlinks, protected and ignored paths, stale hashes, partial operations, and metadata-only telemetry. Add sequential SQL migrations rather than editing migrations that may already have run. See [observer storage](docs/OBSERVER.md#sqlite-storage-and-migrations).

Use `Cargo.lock` and `--locked` for reproducible dependency resolution. Make dependency changes deliberately and explain why an existing dependency or standard-library function is insufficient. Do not introduce a frontend package manager for the static observer UI.

## Verification

Run these commands from the repository root before submitting code changes:

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
node --check observer-ui/app.js
node --check observer-ui/charts.js
```

For focused observer work:

```bash
cargo test --locked --bin file-system-mcp observer::
cargo test --locked --test observer_integration --test tunnel_launcher
```

For UI changes, also verify the dashboard with a disposable workspace and database: keyboard navigation, narrow screens, connection failure, storage warnings, filtering, refresh, and export. Do not send fixture or real credentials to a live tunnel during tests.

## Submitting a pull request

Describe the problem, resulting behavior, tests run, and any compatibility or migration implications. Include the observed failing regression test for a fix, or the existing characterization coverage for a behavior-preserving replacement. State checks that could not run and why.

Keep `.env`, SQLite volumes, exports, credentials, personal paths, and build artifacts out of your changes. The checked-in `.env.example` contains placeholders only. Review the staged diff before pushing. Report suspected vulnerabilities privately according to [SECURITY.md](SECURITY.md).
