use super::scoped_write::scoped_relative;
use super::{MAX_PATCH_CONTENT_SIZE, ServerConfig};
use crate::protocol::ToolCallResult;
use serde_json::Value;

pub(super) fn tool_validate_patch(config: &ServerConfig, args: &Value) -> ToolCallResult {
    match validate_patch(config, args) {
        Ok(output) => ToolCallResult::ok(output),
        Err(error) => ToolCallResult::error(error),
    }
}

fn validate_patch(config: &ServerConfig, args: &Value) -> Result<String, String> {
    let target = args
        .get("target_dir")
        .and_then(Value::as_str)
        .unwrap_or(".");
    let relative_target = patch_directory(config, target)?;
    let (name, content) = patch_input(config, args)?;
    let preview = preview_arguments(args, &relative_target, content);
    super::workspace_tools::preview_patch(config, &preview).map_err(|error| {
        format!(
            "Patch '{}' failed to apply to '{}':\n\n{}",
            name, target, error
        )
    })?;
    Ok(format!(
        "Patch '{}' applies cleanly to '{}' with no conflicts (dry-run check passed).",
        name, target
    ))
}

fn patch_directory(config: &ServerConfig, target: &str) -> Result<std::path::PathBuf, String> {
    let path = config.workspace.legacy_relative(target)?;
    if !config.workspace.metadata(&path)?.is_dir() {
        return Err(format!(
            "Target path '{}' is not a directory. 'target_dir' must be a directory.",
            target
        ));
    }
    Ok(path)
}

fn preview_arguments(args: &Value, target: &std::path::Path, content: String) -> Value {
    let target = if target.as_os_str().is_empty() {
        ".".into()
    } else {
        target.to_string_lossy()
    };
    let mut preview = serde_json::json!({"patch_content": content, "target_dir": target});
    if let Some(hashes) = args.get("expected_hashes") {
        preview["expected_hashes"] = hashes.clone();
    }
    preview
}

fn patch_input(config: &ServerConfig, args: &Value) -> Result<(String, String), String> {
    if let Some(filename) = patch_filename(args) {
        return read_patch(config, filename);
    }
    let content = args
        .get("patch_content")
        .or_else(|| args.get("content"))
        .and_then(Value::as_str)
        .ok_or("Missing or empty required parameter: 'patch_filename'")?;
    check_patch_size(content.len() as u64, "content")?;
    Ok(("<inline_patch>".to_string(), content.to_string()))
}

fn patch_filename(args: &Value) -> Option<&str> {
    args.get("patch_filename")
        .or_else(|| args.get("filename"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|filename| !filename.is_empty())
}

fn read_patch(config: &ServerConfig, filename: &str) -> Result<(String, String), String> {
    let path = scoped_relative(config, &config.patches_dir, filename, true)
        .map_err(|error| format!("Invalid patch_filename: {}", error))?;
    let file = config
        .workspace
        .open_file(&path)
        .map_err(|error| patch_read_error(filename, error))?;
    let content = read_patch_content(file)?;
    Ok((path.display().to_string(), content))
}

fn read_patch_content(file: std::fs::File) -> Result<String, String> {
    let size = file
        .metadata()
        .map_err(|error| format!("Failed to read patch metadata: {}", error))?
        .len();
    check_patch_size(size, "file")?;
    use std::io::Read;
    let mut bytes = Vec::new();
    file.take(MAX_PATCH_CONTENT_SIZE as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Failed to read patch file: {}", error))?;
    check_patch_size(bytes.len() as u64, "file")?;
    String::from_utf8(bytes).map_err(|error| format!("Failed to read patch file: {}", error))
}

fn check_patch_size(size: u64, source: &str) -> Result<(), String> {
    if size <= MAX_PATCH_CONTENT_SIZE as u64 {
        return Ok(());
    }
    Err(format!(
        "Patch {} size ({} bytes) exceeds safety limit of 1 MB ({} bytes).",
        source, size, MAX_PATCH_CONTENT_SIZE
    ))
}

fn patch_read_error(filename: &str, error: String) -> String {
    if error.contains("not found")
        || error.contains("No such file")
        || error.contains("does not exist")
    {
        format!("Patch file '{}' does not exist.", filename)
    } else {
        format!("Invalid patch_filename: {}", error)
    }
}
