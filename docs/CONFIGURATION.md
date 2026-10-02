# Configuration and connections

Build from the repository root:

```bash
cargo build --release --locked
./target/release/file-system-mcp --root "$HOME/Projects/workspace"
```

The workspace must already exist and be a directory. The server supports macOS and Linux; Linux requires `/proc/self/fd`. Its MCP transport is line-delimited JSON-RPC on stdio. An MCP client must start the process and keep stdin open. Startup diagnostics go to stderr.

## Direct stdio configuration

| Setting | CLI flag | Environment variables | Default |
|---|---|---|---|
| Workspace root | `--root <PATH>` | `WORKSPACE_ROOT`, `MCP_WORKSPACE_ROOT` | Discovered root, read-only |
| Plans directory | `--plans-dir <PATH>` | `PLANS_DIR`, `MCP_PLANS_DIR` | `plans`, inside root |
| Patches directory | `--patches-dir <PATH>` | `PATCHES_DIR`, `MCP_PATCHES_DIR` | `patches`, inside root |
| Enable observer | `--dashboard` | — | Disabled |
| Observer port | `--dashboard-port <PORT>` with `--dashboard` | — | `9411` |
| Observer SQLite file | `--dashboard-db <PATH>` with `--dashboard` | `MCP_OBSERVER_DB` | Project `.data/observer.sqlite3` |
| Observer source label | `--observer-source <SOURCE>` with `--dashboard` | `MCP_OBSERVER_SOURCE` | `unknown` |

CLI paths take precedence over environment variables; within each alias pair the first listed environment variable takes precedence. Startup paths support leading tilde expansion. Plain stdio startup does **not** load `.env`, and its port comes from the flag/default, not `MCP_DASHBOARD_PORT`.

The source label accepts `unknown`, `chatgpt`, `chatgpt_work`, `codex`, `codex_cloud`, or `openai_dot`. CLI overrides shell for direct startup. These labels describe configured attribution, not authenticated caller identity; see [origin evidence](OBSERVER.md#source-labels-and-server-runs) for optional per-call tags and their limits.

An explicit root enables mutations. When no nonempty root setting is supplied, discovery remains read-only, including plan and patch-file writes. Discovery chooses the parent when started from a directory named `file-system-mcp`, then considers a parent's `AGENTS.md`, the current directory's `AGENTS.md`, and finally the parent/current directory fallback. Use an explicit root to make deployment behavior predictable and inspect it with `workspace_info`.

Plan/patch directory settings may be relative to the root or absolute, but must remain inside it. Existing symlink components and external directories are rejected at startup. See [tools](TOOLS.md) for operation arguments and path rules, and [observer storage](OBSERVER.md) for volume configuration.

For a stdio client's configuration, set its executable to the absolute path of `target/release/file-system-mcp`, supply `--root` and the absolute workspace path as separate arguments, and provide environment settings through that client. The exact client configuration format depends on the client.

## OpenAI Secure MCP Tunnel

Install and configure the [OpenAI Secure MCP Tunnel](https://developers.openai.com/api/docs/guides/secure-mcp-tunnels) client separately, then ensure `tunnel-client` is on `PATH`. Copy the configuration template and replace its placeholder values locally:

```bash
cp -n .env.example .env
chmod 600 .env
./target/release/file-system-mcp tunnel
```

The launcher loads its project `.env`, infers its own executable path, and launches `tunnel-client run` using `--mcp.command`. The observer starts by default at **http://127.0.0.1:9411**. Keep the command running while using connected tools; Ctrl+C stops it.

Required settings:

| Setting | Accepted names, in priority order |
|---|---|
| Runtime API key | `CONTROL_PLANE_API_KEY`, `OPENAI_API_KEY` |
| Tunnel ID | `CONTROL_PLANE_TUNNEL_ID`, `TUNNEL_ID` |
| Workspace root | `WORKSPACE_ROOT`, `MCP_WORKSPACE_ROOT` |

Existing shell variables take precedence over file values, including aliases. Within each source, names follow the order above. Empty overrides are rejected. Quoted values and `$HOME` expansion use dotenv syntax; no shell script is sourced. Relative workspace paths resolve against the configuration file's directory, and the workspace must already exist.

The launcher locates `.env` beside the nearest ancestor `Cargo.toml` of its executable. A relocated standalone executable without that ancestor uses `.env` beside itself. The launcher therefore works from any current directory; supply its absolute executable path when starting elsewhere. For custom layouts:

```bash
./target/release/file-system-mcp tunnel --env-file /path/to/config.env
```

The configuration file must exist even when required settings are supplied by the shell. The launcher passes the API key through the child process environment, outside command arguments and launcher logs. Keep `.env` private and ignored by Git. Never put real credentials in `.env.example`.

### Observer settings for the tunnel

| Setting | Tunnel CLI | `.env` / shell | Default |
|---|---|---|---|
| Port | `tunnel --dashboard-port <PORT>` | `MCP_DASHBOARD_PORT` | `9411` |
| SQLite file | `tunnel --dashboard-db <PATH>` | `MCP_OBSERVER_DB` | Project `.data/observer.sqlite3` |
| Source label | `tunnel --observer-source <SOURCE>` | `MCP_OBSERVER_SOURCE` | `unknown` |
| Disable observer | `tunnel --no-dashboard` | — | Enabled |

CLI settings take precedence. Source labels use CLI over shell over `.env`, with the same fixed values as direct startup. Port `0` selects a free port and prints the URL to stderr. `--no-dashboard` cannot be combined with observer port/database/source flags and avoids opening telemetry storage. Relative environment/file database paths resolve against the `.env` directory; relative CLI database paths resolve against the launch directory. Each observer covers one process and its database; concurrent servers need separate database paths and ports.

### Tunnel client's health/admin UI

The launcher places the client's health/admin listener on a private Unix socket rather than its default TCP port and disables automatic opening of the client UI. This configuration was verified with `tunnel-client` 0.0.14: its `/ui` handler remains on the socket. The launcher changes listener configuration rather than removing handlers. Health and readiness endpoints remain available.

The socket path is `/tmp/file-system-mcp-<uid>-runtime/tunnel-health-<pid>.sock`. Its parent must belong to the current user with mode `0700`; the client removes its socket on normal shutdown. An already-running tunnel client on port 8080 is unaffected: stop it and restart with the launcher to apply this configuration.

Restart the server/tunnel after rebuilding or changing configuration. In ChatGPT, refresh the connected app's tools after tool changes. Write confirmations are controlled by the client; enabling server-side mutations does not bypass them. See [ChatGPT Developer mode](https://developers.openai.com/api/docs/guides/developer-mode).

## Common startup errors

- Missing or invalid tunnel settings: check the local configuration file and shell overrides without printing credentials.
- Missing workspace: create the intended directory first and confirm the explicit root path.
- Observer volume inside the workspace: choose a private database path outside the root with `--dashboard-db` or `MCP_OBSERVER_DB`.
- Busy dashboard port: stop the process that owns it, choose another port, or use port `0`.
- Database in use: each observer needs its own database and port. Stop the other instance rather than removing its lock file.
- Newer database schema: upgrade the server; older binaries do not downgrade databases. Back up before migrations.
