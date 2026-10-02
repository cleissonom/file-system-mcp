use super::{MAX_PATCH_CONTENT_SIZE, MAX_PLAN_CONTENT_SIZE, ServerConfig};
use crate::protocol::ToolCallResult;
use std::path::{Path, PathBuf};

enum Scope {
    Plan,
    Patch,
}

impl Scope {
    fn directory<'a>(&self, config: &'a ServerConfig) -> &'a Path {
        match self {
            Self::Plan => &config.plans_dir,
            Self::Patch => &config.patches_dir,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Plan => "Plan",
            Self::Patch => "Patch",
        }
    }

    fn maximum(&self) -> usize {
        match self {
            Self::Plan => MAX_PLAN_CONTENT_SIZE,
            Self::Patch => MAX_PATCH_CONTENT_SIZE,
        }
    }

    fn limit_label(&self) -> &'static str {
        match self {
            Self::Plan => "500 KB",
            Self::Patch => "1 MB",
        }
    }
}

pub(super) fn tool_write_plan_file(
    config: &ServerConfig,
    args: &serde_json::Value,
) -> ToolCallResult {
    scoped_result(write_scoped(config, args, Scope::Plan))
}

pub(super) fn tool_write_patch_file(
    config: &ServerConfig,
    args: &serde_json::Value,
) -> ToolCallResult {
    scoped_result(write_scoped(config, args, Scope::Patch))
}

fn scoped_result(result: Result<String, String>) -> ToolCallResult {
    match result {
        Ok(output) => ToolCallResult::ok(output),
        Err(error) => ToolCallResult::error(error),
    }
}

fn write_scoped(
    config: &ServerConfig,
    args: &serde_json::Value,
    scope: Scope,
) -> Result<String, String> {
    let (filename, content, overwrite) = scoped_input(args)?;
    check_content_size(&scope, content)?;
    let path = scoped_relative(
        config,
        scope.directory(config),
        filename,
        matches!(scope, Scope::Patch),
    )?;
    save_scoped(config, &scope, &path, content, overwrite)?;
    Ok(format!(
        "Successfully saved {} to '{}' ({} bytes).",
        scope.label().to_lowercase(),
        path.display(),
        content.len()
    ))
}

fn scoped_input(args: &serde_json::Value) -> Result<(&str, &str, bool), String> {
    let filename = args
        .get("filename")
        .and_then(|value| value.as_str())
        .filter(|name| !name.trim().is_empty())
        .ok_or("Missing or empty required parameter: 'filename'")?;
    let content = args
        .get("content")
        .and_then(|value| value.as_str())
        .ok_or("Missing required parameter: 'content'")?;
    let overwrite = args
        .get("overwrite")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    Ok((filename, content, overwrite))
}

fn save_scoped(
    config: &ServerConfig,
    scope: &Scope,
    path: &Path,
    content: &str,
    overwrite: bool,
) -> Result<(), String> {
    config.workspace.create_parent_directory(path)?;
    config
        .workspace
        .write(path, content.as_bytes(), overwrite, None)
        .map(|_| ())
        .map_err(|error| write_error(scope, path, error))
}

fn check_content_size(scope: &Scope, content: &str) -> Result<(), String> {
    if content.len() <= scope.maximum() {
        return Ok(());
    }
    Err(format!(
        "{} content size ({} bytes) exceeds safety limit of {} ({} bytes).",
        scope.label(),
        content.len(),
        scope.limit_label(),
        scope.maximum()
    ))
}

fn write_error(scope: &Scope, path: &Path, error: String) -> String {
    if error.contains("already exists") {
        format!(
            "{} file '{}' already exists. Set overwrite to true to replace it or choose a different filename.",
            scope.label(),
            path.display()
        )
    } else {
        format!(
            "Failed to write {} file: {}",
            scope.label().to_lowercase(),
            error
        )
    }
}

/// Preserve legacy scoped aliases while keeping the resulting path within the workspace.
pub(super) fn scoped_relative(
    config: &ServerConfig,
    scope_dir: &Path,
    filename: &str,
    is_patch: bool,
) -> Result<PathBuf, String> {
    let root = config.workspace.root();
    let scope = scope_path(config, scope_dir)?;
    let subpath =
        crate::security::normalize_scoped_filename(root, &root.join(&scope), filename, is_patch)
            .map_err(|error| error.to_string())?;
    config
        .workspace
        .relative(&scope.join(subpath).to_string_lossy())
}

fn scope_path(config: &ServerConfig, scope_dir: &Path) -> Result<PathBuf, String> {
    let relative = if scope_dir.is_absolute() {
        scope_dir
            .strip_prefix(config.workspace.root())
            .map_err(|_| "Path is outside the workspace root")?
    } else {
        scope_dir
    };
    let path = if relative.as_os_str().is_empty() {
        ".".into()
    } else {
        relative.to_string_lossy()
    };
    config.workspace.relative(&path)
}
