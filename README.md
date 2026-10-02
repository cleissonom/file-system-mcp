# File System MCP

A Rust MCP server that lets ChatGPT and other MCP clients read, search, create, edit, patch, copy, move, and delete files inside a configured workspace. It exposes 15 tools over stdio and includes a local SQLite-backed usage dashboard.

The workspace root defines the filesystem boundary. Protected paths, Git ignore rules, symlinks, and operation limits restrict access. An explicit root enables mutations; automatic root discovery is read-only. The server runs with its host user's permissions, so review the [security policy](SECURITY.md) before choosing a workspace.

## Requirements

- macOS or Linux. Linux requires `/proc/self/fd`; other operating systems are not supported.
- Rust 1.93.1 or newer and a C compiler to build bundled SQLite.
- A stdio MCP client, or `tunnel-client` on `PATH` for OpenAI Secure MCP Tunnels.

Node.js is not needed to run the server or dashboard. Development checks use Git and Node.js 24. There is no separate database service or frontend build step.

## Quick start

Run from the repository root after downloading or cloning it:

```bash
mkdir -p "$HOME/mcp-workspace"
cargo build --release --locked
./target/release/file-system-mcp --root "$HOME/mcp-workspace"
```

The workspace must exist before startup. Configure your MCP client to launch the compiled executable with `--root` and the absolute workspace path as separate arguments. The process reads one JSON-RPC message per stdin line; stdout is reserved for protocol responses and diagnostics go to stderr. Keep the client connection open while using tools.

To enable the local observer for a direct connection:

```bash
./target/release/file-system-mcp --root "$HOME/mcp-workspace" --dashboard
```

Open **http://127.0.0.1:9411**. Port `0` chooses a free port when supplied with `--dashboard-port 0`.

## Connect through an OpenAI Secure MCP Tunnel

Install the [OpenAI Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels) client separately. For first-time local configuration:

```bash
cp -n .env.example .env
chmod 600 .env
```

Edit `.env` locally, replacing the placeholders:

```dotenv
CONTROL_PLANE_API_KEY="replace-with-your-runtime-api-key"
TUNNEL_ID="replace-with-your-tunnel-id"
WORKSPACE_ROOT="$HOME/mcp-workspace"
```

Then start:

```bash
./target/release/file-system-mcp tunnel
```

The launcher reads the project's `.env`, infers its own executable location, and starts the tunnel plus observer dashboard. It works from other directories when invoked with its absolute path. Shell settings override `.env`; the API key travels through the child process environment. Keep `.env` private and out of Git.

The tunnel client's health/admin listener uses a private Unix socket instead of port 8080. Its admin UI handler remains available on that socket with the tested client; automatic UI opening is disabled. Client-side write confirmations still apply. See [configuration and connections](docs/CONFIGURATION.md) for aliases, precedence, custom paths, and troubleshooting.

## Tools

| Purpose | Tools |
|---|---|
| Inspect and read | `workspace_info`, `file_info`, `read_file`, `list_directory`, `search_files` |
| Create and modify | `write_file`, `edit_file`, `apply_patch`, `create_directory`, `copy_path`, `move_path`, `delete_path` |
| Plans and patches | `write_plan_file`, `write_patch_file`, `validate_patch` |

Writes require an explicit root. Text edits support expected SHA-256 hashes to detect stale content. File publication is atomic per file; multi-file patches and recursive deletion can partially complete after an I/O failure. No shell-command execution tool is exposed. See the [tool reference](docs/TOOLS.md) for arguments, examples, limits, and boundary details.

## Observer and local data

The dashboard shows tool calls, outcomes, latency, bytes, all in-flight requests, source labels with attribution evidence, anonymous server runs, and recent activity. Uptime and active elapsed time update every second between two-second snapshots, freezing when paused or disconnected until a fresh snapshot succeeds. Completed-call metadata is saved to `.data/observer.sqlite3` beside the project and restored on restart. Startup initializes embedded sequential SQL migrations automatically.

Set `--observer-source codex` with `--dashboard`, or `MCP_OBSERVER_SOURCE`, to label a dedicated connection; `tunnel --observer-source chatgpt_work` labels a tunnel launch. The default is `unknown`. Labels are unverified attribution, not authenticated caller identity. ChatGPT does not automatically provide the custom per-call source tag described in the [observer guide](docs/OBSERVER.md#source-labels-and-server-runs).

Arguments, returned content, error messages, filenames, workspace paths, raw request IDs, raw client/identity metadata, raw environment values, and credentials are not stored or exported. Fixed typed origin fields and an anonymous run ID are allowed metadata. The database must remain outside the workspace. Security counters cover at most 1,000 retained completed calls, while origin totals cover saved history. Chart views cover 60 minutes; the database grows with usage. Each dashboard observes one process and its database; concurrent processes need separate databases and ports. The observer binds only to local IPv4 loopback and has no login. See [observer and storage](docs/OBSERVER.md) for settings, backups, and failure behavior.

## Development and publication

Run repository checks from this directory:

```bash
cargo fmt --check
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo build --release --locked
node --check observer-ui/app.js
node --check observer-ui/charts.js
node --check observer-ui/activity.js
node --test tests/observer_ui.test.mjs
```

Tests use temporary data, synthetic credentials, and a fake tunnel client. They run headlessly without a real tunnel or API key after dependencies are available.

- [Contributing](CONTRIBUTING.md): development workflow and verification.
- [Repository instructions](AGENTS.md): architecture and engineering conventions.
- [Security policy](SECURITY.md): boundaries, private data, and vulnerability reporting.
- [Publication checklist](docs/PUBLISHING.md): secret review and GitHub preparation.

Licensed under the [MIT License](LICENSE).
