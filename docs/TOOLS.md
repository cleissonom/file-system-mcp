# Tool reference

The server exposes 15 tools over JSON-RPC 2.0 on stdio. Read-only tools advertise `readOnlyHint: true`; mutation tools advertise `false`. Tool results use MCP text content; failures set `isError: true`. Workspace inspection and CRUD operations return JSON objects in that text content and reject unknown argument fields. Read, search, plan, and stored-patch schemas retain their existing contracts.

An explicit `--root`, `WORKSPACE_ROOT`, or `MCP_WORKSPACE_ROOT` enables mutations. Without one, discovery is read-only and every write operation is rejected, including plan and patch-file writes. Call `workspace_info` to inspect the effective root and limits. See [configuration](CONFIGURATION.md).

## Available tools

| Tool | Arguments | Behavior |
|---|---|---|
| `workspace_info` | None | Reports root, writable state, protected paths, and limits. |
| `file_info` | `path` | Reports type, byte size, Unix modification time, and SHA-256 `content_hash`. Hash is `null` for directories and files larger than 1 MiB. |
| `write_file` | `path`, `content`; optional `overwrite`, `expected_hash` | Creates or atomically replaces a UTF-8 text file. Parent must exist. Replacing a file requires `overwrite: true`. Returns path, bytes written, and content hash. |
| `edit_file` | `path`, `edits`; optional `expected_hash` | Applies ordered exact-text replacements in memory, then atomically publishes the result. Each edit has `old_text`, `new_text`, and optional `replace_all`. Empty, missing, or ambiguous matches are rejected. |
| `apply_patch` | `patch_content`; optional `target_dir`, `expected_hashes` | Preflights every text-diff target, hash, and hunk, then creates, modifies, or deletes files. Target directory defaults to `.`. |
| `create_directory` | `path`; optional `recursive` | Creates a directory. `recursive: true` creates missing parents and permits an existing directory. Validates the full path before creating parents. |
| `copy_path` | `source`, `destination`; optional `overwrite`, `recursive` | Copies a regular file or directory. Directory copy requires `recursive: true` and a new destination. Files can be replaced with `overwrite: true`. Preserves ordinary permission bits. |
| `move_path` | `source`, `destination`; optional `overwrite`, `recursive` | Renames a file or directory. Replacing an existing file requires `overwrite: true`; existing directories cannot be replaced. All descendants are checked regardless of the compatibility `recursive` flag. Cross-filesystem moves fail. |
| `delete_path` | `path`; optional `recursive` | Deletes a file or empty directory. Nonempty directories require `recursive: true`. Validates all descendants before recursive deletion. |
| `read_file` | `path`; optional `offset`, `limit` | Reads paginated text, omitting binary content. Defaults to 25,000 bytes, maximum 100,000. |
| `list_directory` | Optional `path`, `depth`, `max_entries` | Lists allowed entries. Defaults: `.`, depth 1, 200 entries. Maximum depth 5 and 1,000 entries. |
| `search_files` | `query`; optional `path`, `is_regex`, `file_pattern`, `max_results` | Searches allowed text files. Defaults to 50 matching lines, maximum 200. Skips binary files and files larger than 5 MiB. |
| `write_plan_file` | `filename`, `content`; optional `overwrite` | Writes `.md`/`.markdown` under the configured plans directory, creating allowed parents as needed. Maximum 500 KiB. |
| `write_patch_file` | `filename`, `content`; optional `overwrite` | Writes `.patch`/`.diff` under the configured patches directory, creating allowed parents as needed. Maximum 1 MiB. |
| `validate_patch` | `patch_filename` or `patch_content`; optional `target_dir`, `expected_hashes` | Uses the same confined text-diff preflight as `apply_patch`, without modifying files or creating directories. |

## Create and edit a file

These examples are the `params` objects for a `tools/call` request. Create a parent directory:

```json
{"name":"create_directory","arguments":{"path":"notes","recursive":true}}
```

Create the file:

```json
{"name":"write_file","arguments":{"path":"notes/task.md","content":"# Task\nDraft\n"}}
```

Inspect it with `file_info`, then use the returned `content_hash` to reject a stale edit:

```json
{
  "name": "edit_file",
  "arguments": {
    "path": "notes/task.md",
    "expected_hash": "<SHA-256 returned by file_info>",
    "edits": [{"old_text": "Draft", "new_text": "Ready"}]
  }
}
```

## Patching

Supply a standard unified text diff. Paths are relative to `target_dir`, which itself is relative to the workspace root. Regular Git text-diff headers are accepted; `/dev/null` denotes creation or deletion. Parent directories for new files must already exist.

```json
{
  "name": "apply_patch",
  "arguments": {
    "target_dir": "notes",
    "patch_content": "--- a/task.md\n+++ b/task.md\n@@ -1,2 +1,2 @@\n # Task\n-Ready\n+Done\n"
  }
}
```

Optional `expected_hashes` maps existing patch targets, relative to `target_dir`, to SHA-256 hashes. Creation targets cannot have an expected hash. Even without caller hashes, edits and patches recheck the content read during preparation before publishing each changed file.

The parser accepts UTF-8 text changes, at most 100 targets and 1 MiB of diff content. It rejects traversal, absolute paths, duplicate targets including existing filesystem aliases, ambiguous quoted paths, renames/copies, mode-only changes, binary diffs, and symlink diffs. Use `move_path` for renames. Regular `100644` new/deleted-file framing is accepted; mode-changing operations are not exposed. `validate_patch` uses this parser, not `git apply --check`, and rejects unsupported Git patches in the same way.

Every target and hunk is validated before the first mutation. Publication is atomic per file, **not transactional across files**. A later I/O or concurrent-change failure can leave earlier files changed. Two absent creation targets that alias one another on the filesystem can collide at atomic no-replace publication. Errors contain `changed_paths`, `failed_path`, `failed_path_may_have_changed`, and `multi_file_atomic: false`; inspect the workspace before retrying. Recursive deletion can also partially complete after an I/O failure and reports the number of entries removed.

## Workspace boundary

Workspace inspection and CRUD paths must be relative to the configured root. `.` is allowed for inspecting the root, but the root itself cannot be written, copied, moved, or deleted. Absolute paths, `..`, NUL/backslash characters, symlink components, and nonregular filesystem objects are rejected. Read, plan, and stored-patch tools also accept their existing rooted aliases, resolved through the same workspace backend.

Transfers reject paths that resolve to the same entry or contain one another, including case/Unicode aliases. A case-only rename on a case-insensitive volume requires moving through a distinct intermediate name. Distinct hard-link names remain separate paths for copies and patches; atomic replacement intentionally separates their contents. Moves between hard links to the same file are rejected because the operating system would leave the source in place.

Every path is checked against the security denylist and scoped Git ignore rules. Protected paths include `.git`, `.env*`, `.ssh`, `.aws`, SSH private-key names, `*.pem`, `*.key`, their suffixed forms, and `bastion.sh`. Temporary `.mcp-tmp-*` staging names are reserved. Copying, moving, or recursively deleting a directory containing protected, ignored, or symlink descendants is rejected before changes begin. This includes complete Git repository directories containing `.git`.

Ignore rules come from workspace and repository `.gitignore` files, nested `.gitignore` files, and repository `.git/info/exclude`, read through confined handles. Global ignore files outside the workspace are not loaded. Rules apply to both existing and newly created paths.

Operations use an open root-directory handle, walk components with `O_NOFOLLOW`, and operate relative to opened parent handles. Policy checks use actual handle names to prevent case/Unicode aliases from bypassing rules. macOS uses `F_GETPATH`; Linux requires trusted live-descriptor metadata under `/proc/self/fd` and fails closed if it is unavailable. File replacement leaves outside hard-linked contents unchanged. Existing ordinary permission bits are preserved; new text files start with owner-only permissions. Directory copies are staged before publication. `overwrite: false` uses atomic no-replace rename, so a destination appearing during publication is not overwritten.

Limits bound text writes/edits to 1 MiB, copies to 100 MiB of actual aggregate file bytes, and traversal to 10,000 entries and 64 levels. Read listings/searches may truncate traversal. Copied directories preserve ordinary permissions without copying ownership, ACLs, extended attributes, or special set-ID bits.

The server runs with host-user permissions, not an operating-system sandbox. Other host processes can rename an already-open ancestor outside the root. Expected hashes do not provide atomic compare-and-swap against uncooperative writers. Avoid concurrent external directory mutation during operations. No shell execution tool is exposed. Read the [security policy](../SECURITY.md) before using sensitive workspaces.
