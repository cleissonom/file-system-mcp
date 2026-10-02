use super::ServerConfig;
use super::workspace_args::*;
use crate::protocol::ToolCallResult;
use crate::unified_patch::{ChangeKind, MAX_PATCH_BYTES, MAX_PATCH_FILES, PatchChange};
use crate::workspace::{
    MAX_COPY_BYTES, MAX_TEXT_BYTES, MAX_TREE_DEPTH, MAX_TREE_ENTRIES, content_hash,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

pub(super) fn execute(config: &ServerConfig, name: &str, args: &Value) -> ToolCallResult {
    if is_mutation(name) && !config.writable {
        return ToolCallResult::error("Writes require an explicit WORKSPACE_ROOT or --root.");
    }
    match dispatch(config, name, args) {
        Ok(result) => ToolCallResult::ok(result.to_string()),
        Err(message) => ToolCallResult::error(message),
    }
}

fn is_mutation(name: &str) -> bool {
    matches!(
        name,
        "write_file"
            | "edit_file"
            | "apply_patch"
            | "create_directory"
            | "copy_path"
            | "move_path"
            | "delete_path"
    )
}

fn dispatch(config: &ServerConfig, name: &str, args: &Value) -> Result<Value, String> {
    match name {
        "workspace_info" => workspace_info(config, args),
        "file_info" => file_info(config, decode(args)?),
        "write_file" => write_file(config, decode(args)?),
        "edit_file" => edit_file(config, decode(args)?),
        "apply_patch" => apply_patch(config, decode(args)?),
        "create_directory" => create_directory(config, decode(args)?),
        "copy_path" => copy_path(config, decode(args)?),
        "move_path" => move_path(config, decode(args)?),
        "delete_path" => delete_path(config, decode(args)?),
        _ => Err(format!("Unknown tool: '{name}'")),
    }
}

fn decode<T: DeserializeOwned>(args: &Value) -> Result<T, String> {
    serde_json::from_value(args.clone()).map_err(|error| format!("Invalid arguments: {error}"))
}

fn workspace_info(config: &ServerConfig, args: &Value) -> Result<Value, String> {
    let _: EmptyArgs = decode(args)?;
    Ok(json!({
        "root": config.workspace.root(), "writable": config.writable,
        "limits": {"max_text_bytes": MAX_TEXT_BYTES, "max_copy_bytes": MAX_COPY_BYTES,
            "max_tree_entries": MAX_TREE_ENTRIES, "max_tree_depth": MAX_TREE_DEPTH,
            "max_patch_bytes": MAX_PATCH_BYTES, "max_patch_files": MAX_PATCH_FILES},
        "protected_paths": [".git/", ".ssh/", ".aws/", ".env*", "*.pem", "*.key",
            "*.pem.*", "*.key.*", "id_rsa", "id_ed25519", "id_dsa", "id_ecdsa",
            "bastion.sh", "bastion.sh.*"],
        "gitignore_respected": true, "symlinks_allowed": false,
        "multi_file_atomic": false
    }))
}

fn file_info(config: &ServerConfig, args: PathArgs) -> Result<Value, String> {
    let path = config.workspace.relative(&args.path)?;
    let metadata = config.workspace.metadata(&path)?;
    if !metadata.is_file() && !metadata.is_dir() {
        return Err("Only regular files and directories are supported.".to_string());
    }
    let hash = file_info_hash(config, &path, &metadata)?;
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|time| time.as_secs());
    Ok(json!({
        "path": path, "type": if metadata.is_dir() { "directory" } else { "file" },
        "size": metadata.len(), "modified": modified, "content_hash": hash
    }))
}

fn file_info_hash(
    config: &ServerConfig,
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Result<Option<String>, String> {
    if metadata.is_dir() || metadata.len() > MAX_TEXT_BYTES as u64 {
        return Ok(None);
    }
    Ok(Some(content_hash(
        &config.workspace.read(path, MAX_TEXT_BYTES)?,
    )))
}

fn write_file(config: &ServerConfig, args: WriteArgs) -> Result<Value, String> {
    check_text_size(args.content.len())?;
    let path = config.workspace.relative(&args.path)?;
    let expected = args
        .expected_hash
        .as_deref()
        .map(validate_hash)
        .transpose()?;
    let hash = config.workspace.write(
        &path,
        args.content.as_bytes(),
        args.overwrite,
        expected.as_deref(),
    )?;
    Ok(write_result(&path, &hash, args.content.len()))
}

fn write_result(path: &Path, hash: &str, bytes_written: usize) -> Value {
    json!({"path": path, "content_hash": hash, "bytes_written": bytes_written})
}

fn edit_file(config: &ServerConfig, args: EditArgs) -> Result<Value, String> {
    if args.edits.is_empty() {
        return Err("edits must contain at least one replacement.".to_string());
    }
    let path = config.workspace.relative(&args.path)?;
    let original = read_text(config, &path)?;
    let original_hash = content_hash(original.as_bytes());
    check_expected_hash(args.expected_hash.as_deref(), &original_hash)?;
    let updated = apply_edits(original, args.edits)?;
    let hash = config
        .workspace
        .write(&path, updated.as_bytes(), true, Some(&original_hash))?;
    Ok(write_result(&path, &hash, updated.len()))
}

fn read_text(config: &ServerConfig, path: &Path) -> Result<String, String> {
    let bytes = config.workspace.read(path, MAX_TEXT_BYTES)?;
    String::from_utf8(bytes).map_err(|error| format!("File is not valid UTF-8: {error}"))
}

fn apply_edits(mut content: String, edits: Vec<TextEdit>) -> Result<String, String> {
    for (index, edit) in edits.into_iter().enumerate() {
        let count = validate_edit(&content, &edit)
            .map_err(|error| format!("Edit {}: {error}", index + 1))?;
        let new_size = content.len() - count * edit.old_text.len();
        let size = edit
            .new_text
            .len()
            .checked_mul(count)
            .and_then(|n| new_size.checked_add(n))
            .ok_or_else(|| "Edited content exceeds the text limit.".to_string())?;
        check_text_size(size)?;
        content = content.replace(&edit.old_text, &edit.new_text);
    }
    Ok(content)
}

fn validate_edit(content: &str, edit: &TextEdit) -> Result<usize, String> {
    if edit.old_text.is_empty() {
        return Err("old_text must not be empty.".to_string());
    }
    let count = content.matches(&edit.old_text).count();
    if count == 0 {
        return Err("old_text has no matching text.".to_string());
    }
    if count > 1 && !edit.replace_all {
        return Err(format!(
            "old_text has {count} matches; use replace_all=true or provide unique text."
        ));
    }
    Ok(count)
}

fn check_text_size(bytes: usize) -> Result<(), String> {
    if bytes > MAX_TEXT_BYTES {
        return Err(format!(
            "Content exceeds the {MAX_TEXT_BYTES}-byte text limit."
        ));
    }
    Ok(())
}

fn validate_hash(hash: &str) -> Result<String, String> {
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("expected_hash must be a 64-character hexadecimal SHA-256 hash.".to_string());
    }
    Ok(hash.to_ascii_lowercase())
}

fn check_expected_hash(expected: Option<&str>, actual: &str) -> Result<(), String> {
    if let Some(hash) = expected
        && validate_hash(hash)? != actual
    {
        return Err("Content hash mismatch; the file changed since it was inspected.".to_string());
    }
    Ok(())
}

fn create_directory(config: &ServerConfig, args: DirectoryArgs) -> Result<Value, String> {
    let path = config.workspace.relative(&args.path)?;
    config.workspace.create_directory(&path, args.recursive)?;
    Ok(json!({"path": path, "created": true}))
}

fn copy_path(config: &ServerConfig, args: CopyArgs) -> Result<Value, String> {
    let source = config.workspace.relative(&args.source)?;
    let destination = config.workspace.relative(&args.destination)?;
    config
        .workspace
        .copy(&source, &destination, args.overwrite, args.recursive)?;
    Ok(json!({"source": source, "destination": destination, "copied": true}))
}

fn move_path(config: &ServerConfig, args: MoveArgs) -> Result<Value, String> {
    let source = config.workspace.relative(&args.source)?;
    let destination = config.workspace.relative(&args.destination)?;
    config
        .workspace
        .move_path(&source, &destination, args.overwrite)?;
    Ok(json!({"source": source, "destination": destination, "moved": true}))
}

fn delete_path(config: &ServerConfig, args: DirectoryArgs) -> Result<Value, String> {
    let path = config.workspace.relative(&args.path)?;
    config.workspace.delete(&path, args.recursive)?;
    Ok(json!({"path": path, "deleted": true}))
}

struct PreparedChange {
    path: PathBuf,
    kind: ChangeKind,
    content: String,
    original_hash: Option<String>,
}

pub(super) fn preview_patch(config: &ServerConfig, args: &Value) -> Result<Vec<PathBuf>, String> {
    let prepared = prepare_patch(config, decode(args)?)?;
    Ok(prepared.into_iter().map(|change| change.path).collect())
}

fn apply_patch(config: &ServerConfig, args: PatchArgs) -> Result<Value, String> {
    let prepared = prepare_patch(config, args)?;
    let mut changed_paths = Vec::new();
    for change in prepared {
        if let Err(error) = publish_change(config, &change) {
            return Err(json!({"error": error, "changed_paths": changed_paths,
                "failed_path": change.path, "failed_path_may_have_changed": true,
                "multi_file_atomic": false})
            .to_string());
        }
        changed_paths.push(change.path);
    }
    Ok(json!({"changed_paths": changed_paths, "multi_file_atomic": false}))
}

fn prepare_patch(config: &ServerConfig, args: PatchArgs) -> Result<Vec<PreparedChange>, String> {
    check_text_size(args.patch_content.len())?;
    let target = config.workspace.relative(&args.target_dir)?;
    if !config.workspace.metadata(&target)?.is_dir() {
        return Err("target_dir must be an existing directory.".to_string());
    }
    let changes = crate::unified_patch::parse(&args.patch_content)?;
    validate_hash_targets(&args, &changes)?;
    let mut prepared = Vec::with_capacity(changes.len());
    let mut targets = BTreeSet::new();
    for change in changes {
        let expected = args
            .expected_hashes
            .get(&change.path.to_string_lossy().into_owned());
        let change = prepare_change(config, &target, change, expected.map(String::as_str))?;
        if !targets.insert(change.path.clone()) {
            return Err(format!(
                "Duplicate patch target after filesystem name resolution: {}",
                change.path.display()
            ));
        }
        prepared.push(change);
    }
    Ok(prepared)
}

fn validate_hash_targets(args: &PatchArgs, changes: &[PatchChange]) -> Result<(), String> {
    let paths: BTreeSet<_> = changes
        .iter()
        .filter(|change| change.kind != ChangeKind::Create)
        .map(|change| change.path.to_string_lossy().into_owned())
        .collect();
    for path in args.expected_hashes.keys() {
        if !paths.contains(path) {
            return Err(format!(
                "expected_hashes key '{path}' must name an existing patch target."
            ));
        }
    }
    Ok(())
}

fn prepare_change(
    config: &ServerConfig,
    target: &Path,
    change: PatchChange,
    expected: Option<&str>,
) -> Result<PreparedChange, String> {
    let path = patch_target(config, target, &change.path)?;
    let before = patch_before(config, &path, change.kind)?;
    let original_hash =
        (change.kind != ChangeKind::Create).then(|| content_hash(before.as_bytes()));
    if let Some(hash) = &original_hash {
        check_expected_hash(expected, hash)?;
    }
    let content = change
        .apply_to(&before)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    check_text_size(content.len())?;
    Ok(PreparedChange {
        path,
        kind: change.kind,
        content,
        original_hash,
    })
}

fn patch_target(config: &ServerConfig, target: &Path, path: &Path) -> Result<PathBuf, String> {
    let relative = config
        .workspace
        .relative(path.to_str().ok_or("Patch path is not UTF-8.")?)?;
    let joined = target.join(relative);
    config.workspace.actual_path(&joined)
}

fn patch_before(config: &ServerConfig, path: &Path, kind: ChangeKind) -> Result<String, String> {
    if kind != ChangeKind::Create {
        return read_text(config, path);
    }
    let parent = path
        .parent()
        .ok_or("Patch cannot create the workspace root.")?;
    if !config.workspace.metadata(parent)?.is_dir() {
        return Err("Patch file parent must be an existing directory.".to_string());
    }
    if config.workspace.exists(path)? {
        return Err(format!(
            "Patch creation target already exists: {}",
            path.display()
        ));
    }
    Ok(String::new())
}

fn publish_change(config: &ServerConfig, change: &PreparedChange) -> Result<(), String> {
    if change.kind == ChangeKind::Delete {
        let hash = change
            .original_hash
            .as_deref()
            .ok_or("Deletion is missing its original hash.")?;
        return config.workspace.delete_checked(&change.path, hash);
    }
    config.workspace.write(
        &change.path,
        change.content.as_bytes(),
        change.kind == ChangeKind::Modify,
        change.original_hash.as_deref(),
    )?;
    Ok(())
}
