# Security policy

This server gives an MCP client access to a configured workspace. Choose a workspace whose allowed files the client may read and modify, and run the server with a host account that has only the filesystem permissions it needs. An explicit root enables all supported mutation tools; the server does not provide per-tool authorization controls. Client-side confirmations remain the client's responsibility.

## Workspace boundary

The filesystem backend supports macOS and Linux. It opens the workspace root and walks components through directory handles with no-follow operations. Linux requires `/proc/self/fd` to inspect the actual names of live handles and fails closed when that information is unavailable. Symlink components, path traversal, absolute paths for the workspace tools, and nonregular filesystem objects are rejected. The root itself cannot be written, moved, copied, or deleted.

Protected paths include `.git`, `.env*`, `.ssh`, `.aws`, private SSH key names, `*.pem`, `*.key`, their suffixed variants, and `bastion.sh`. Reserved `.mcp-tmp-*` names cannot be accessed through tools. Scoped Git ignore rules also restrict reads and writes. Recursive copy, move, and delete preflight descendants and reject protected, ignored, or symlink entries. See the [tool reference](docs/TOOLS.md#workspace-boundary) for details and limits.

The denylist protects common credential locations; it cannot identify every secret stored in an ordinary allowed file. Keep secrets outside the workspace or exclude them with workspace-local ignore rules. `.env.example` is safe to publish as a template but is still protected from workspace tools because its filename starts with `.env`.

The server runs with its host user's permissions and is not an operating-system sandbox. Another host process with sufficient permissions can rename an already-open ancestor outside the workspace. Avoid concurrent external directory mutation during operations. Expected hashes are optimistic conflict checks, not an atomic compare-and-swap against uncooperative host writers. No shell-command execution tool is exposed.

Atomic publication applies per file. Multi-file patches and recursive deletion may partially complete after an I/O or concurrent-change failure; inspect the structured error and workspace before retrying. A host process with access to allowed workspace files can also modify security policy files; the server does not protect against a compromised host account.

## Configuration and tunnel credentials

Put tunnel credentials in a local `.env` or the process environment. Use placeholders in `.env.example`. Never commit real credentials, log environment dumps, or include credentials in bug reports.

The tunnel launcher passes the API key through the child process environment instead of command arguments. It does not source shell scripts and does not print key values in configuration errors. The tunnel ID is passed as a client argument. Processes running as the same host user may have access to process configuration; use normal operating-system account isolation where needed.

The launcher places the tunnel client's health/admin listener on a Unix socket in an owned `0700` directory and suppresses automatic opening of its UI. It does not remove the tunnel client's `/ui` handler or reconfigure an already-running client. Authentication and remote tunnel access are provided by the tunnel client and its service, not by this stdio server.

## Observer and stored data

The observer binds to IPv4 loopback only. Its endpoints are read-only and require matching local Host/Origin values; it does not enable CORS. It has no login and is intended for local use. Do not expose it through a public proxy or tunnel. Other local processes that can connect to the listener can read its metadata.

SQLite stores completed-call metadata only: generated sequence, known tool name, outcome, timestamps, duration, byte counts, fixed typed origin fields, and an anonymous server-run ID. Arguments, returned content, error messages, filenames, workspace paths, raw request IDs, raw client/identity metadata, raw environment values, and credentials are not stored or exported. Unknown tool names are normalized to `unknown_tool`. Metadata can still reveal activity patterns and should be treated as local private data. Standard MCP results and startup diagnostics are separate from this telemetry policy.

Source labels are `unknown`, `chatgpt`, `chatgpt_work`, `codex`, `codex_cloud`, or `openai_dot`, with fixed attribution evidence and launch transport. Operator-configured and client-reported labels are unverified, not authenticated identity or authorization. A malformed per-call source tag becomes unknown rather than falling back to a configured label. The server does not infer callers from `clientInfo`, user-agent, or session hints and never retains raw identity metadata. `session_id` identifies an anonymous server run, not a user or conversation. See [origin evidence](docs/OBSERVER.md#source-labels-and-server-runs).

Dashboard security counters cover at most 1,000 retained completed calls; origin totals cover saved history. Read/write capability comes from the tool catalog and does not prove that a call mutated files. One dashboard covers one process and database, with no central aggregation across servers.

The database must be outside the workspace. New database and lock files use owner-only permissions; unsafe ownership, permissions, symlinks, and hard links are rejected. The database, SQLite companions, and adjacent lock file are local artifacts and must remain ignored by Git. Back up with the server stopped or through SQLite backup tooling; do not publish production databases or exports.

## Reporting a vulnerability

Do not open a public issue containing credentials, private files, or an exploitable vulnerability. When the GitHub repository has private vulnerability reporting enabled, use **Security → Report a vulnerability** to contact its maintainers privately. This document does not imply that reporting has already been enabled.

If private reporting is unavailable, ask maintainers in a public issue for a private reporting channel without including exploit details or sensitive data. Include the affected version or commit, operating system, a minimal reproduction with synthetic data, expected behavior, and impact in the private report. If a credential has been exposed, revoke or rotate it through its issuer.

Fixes are made against the current codebase; no long-term support or response-time commitment is established. GitHub private vulnerability reporting is available for public repositories. Maintainers should enable it immediately after changing visibility to public and verify the reporting button before inviting reports. See [GitHub's repository configuration guide](https://docs.github.com/en/code-security/how-tos/report-and-fix-vulnerabilities/configure-vulnerability-reporting/configure-for-a-repository).
