use crate::protocol::Tool;
use crate::workspace::MAX_TEXT_BYTES;
use serde_json::{Value, json};

pub(super) fn get_workspace_tools() -> Vec<Tool> {
    vec![
        Tool::new(
            "workspace_info",
            "Inspect the effective workspace root, write permission, security policy, and limits.",
            object(json!({}), &[]),
            true,
            false,
        ),
        Tool::new(
            "file_info",
            "Inspect a workspace file or directory. Content hashes are SHA-256; directories and files above the text limit have a null hash. Modified times use Unix seconds.",
            object(json!({"path": path()}), &["path"]),
            true,
            false,
        ),
        Tool::new(
            "write_file",
            "Atomically create or replace a UTF-8 text file within the workspace. Parents must exist; replacement requires overwrite=true. Expected hashes prevent stale updates.",
            write_schema(),
            false,
            true,
        ),
        Tool::new(
            "edit_file",
            "Apply ordered exact text replacements in memory, then atomically publish the result. Missing or ambiguous matches reject every edit. Parents and the file must already exist.",
            edit_schema(),
            false,
            true,
        ),
        Tool::new(
            "apply_patch",
            "Apply a unified text diff within the workspace after validating every path, hash, and hunk. Supports file creation, modification, and deletion. I/O failures report changed_paths; multi-file changes are not transactional.",
            patch_schema(),
            false,
            true,
        ),
        Tool::new(
            "create_directory",
            "Create a workspace directory. Set recursive=true to create missing parents.",
            directory_schema(),
            false,
            false,
        ),
        Tool::new(
            "copy_path",
            "Copy a workspace file or directory. Directories require recursive=true and a new destination. Regular-file replacement requires overwrite=true. Protected descendants are rejected before copying.",
            transfer_schema(true),
            false,
            true,
        ),
        Tool::new(
            "move_path",
            "Move or rename a workspace file or directory. Regular-file replacement requires overwrite=true; directory destinations must be new. Workspace root, protected descendants, symlinks, and cross-filesystem moves are rejected.",
            transfer_schema(false),
            false,
            true,
        ),
        Tool::new(
            "delete_path",
            "Delete a workspace file or empty directory. Nonempty directory deletion requires recursive=true. Workspace root and protected descendants cannot be deleted.",
            directory_schema(),
            false,
            true,
        ),
    ]
}

fn object(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object", "properties": properties, "required": required,
        "additionalProperties": false
    })
}

fn path() -> Value {
    json!({"type": "string", "description": "Path relative to WORKSPACE_ROOT; absolute paths, traversal, symlinks, secrets, Git metadata, and gitignored paths are rejected."})
}

fn boolean(description: &str) -> Value {
    json!({"type": "boolean", "default": false, "description": description})
}

fn hash() -> Value {
    json!({"type": "string", "pattern": "^[a-fA-F0-9]{64}$", "description": "Expected SHA-256 of the current file; mismatches reject the operation."})
}

fn text(description: &str) -> Value {
    json!({"type": "string", "description": description, "maxLength": MAX_TEXT_BYTES})
}

fn write_schema() -> Value {
    object(
        json!({
            "path": path(), "content": text("UTF-8 file content, at most 1 MiB in bytes."),
            "overwrite": boolean("Permit replacing an existing regular file."),
            "expected_hash": hash()
        }),
        &["path", "content"],
    )
}

fn edit_schema() -> Value {
    let edit = object(
        json!({
            "old_text": text("Nonempty exact text to match."),
            "new_text": text("Replacement text."),
            "replace_all": boolean("Replace all non-overlapping matches; otherwise exactly one match is required.")
        }),
        &["old_text", "new_text"],
    );
    object(
        json!({
            "path": path(), "expected_hash": hash(),
            "edits": {"type": "array", "minItems": 1, "items": edit}
        }),
        &["path", "edits"],
    )
}

fn patch_schema() -> Value {
    object(
        json!({
            "patch_content": text("Unified diff content, at most 1 MiB in bytes."),
            "target_dir": {"type": "string", "default": ".", "description": "Existing directory relative to WORKSPACE_ROOT. Patch paths are relative to it."},
            "expected_hashes": {"type": "object", "additionalProperties": hash(), "description": "Optional expected SHA-256 hashes keyed by paths relative to target_dir. Keys must name existing patch targets."}
        }),
        &["patch_content"],
    )
}

fn directory_schema() -> Value {
    object(
        json!({
            "path": path(),
            "recursive": boolean("Permit processing descendants or creating missing parent directories.")
        }),
        &["path"],
    )
}

fn transfer_schema(copy: bool) -> Value {
    let recursive = if copy {
        "Permit copying a directory and its descendants."
    } else {
        "Optional compatibility flag; moves always validate every descendant."
    };
    object(
        json!({
            "source": path(), "destination": path(),
            "overwrite": boolean("Permit replacing an existing regular-file destination. Directory destinations must be new."),
            "recursive": boolean(recursive)
        }),
        &["source", "destination"],
    )
}
