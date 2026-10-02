use crate::protocol::{Tool, ToolCallResult};
use crate::workspace::Workspace;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

mod patch_validate;
mod read_tools;
mod schema;
mod scoped_write;
mod workspace_args;
mod workspace_schema;
mod workspace_tools;

use patch_validate::tool_validate_patch;
use read_tools::{tool_list_directory, tool_read_file, tool_search_files};
pub use schema::get_available_tools;
use scoped_write::{tool_write_patch_file, tool_write_plan_file};

pub const DEFAULT_MAX_FILE_SIZE: usize = 25_000;
pub const MAX_ALLOWED_FILE_LIMIT: usize = 100_000;
pub const DEFAULT_MAX_LIST_ENTRIES: usize = 200;
pub const MAX_ALLOWED_LIST_ENTRIES: usize = 1000;
pub const DEFAULT_MAX_SEARCH_RESULTS: usize = 50;
pub const MAX_ALLOWED_SEARCH_RESULTS: usize = 200;
pub const MAX_PLAN_CONTENT_SIZE: usize = 500 * 1024;
pub const MAX_PATCH_CONTENT_SIZE: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub root: std::path::PathBuf,
    pub plans_dir: std::path::PathBuf,
    pub patches_dir: std::path::PathBuf,
    pub writable: bool,
    pub workspace: Arc<Workspace>,
}

impl ServerConfig {
    pub fn new(
        root: std::path::PathBuf,
        plans_dir: std::path::PathBuf,
        patches_dir: std::path::PathBuf,
    ) -> Result<Self, String> {
        let workspace = Arc::new(Workspace::open(&root)?);
        let plans_dir = normalize_scope(&workspace, &root, &plans_dir)?;
        let patches_dir = normalize_scope(&workspace, &root, &patches_dir)?;
        Ok(Self {
            root: workspace.root().to_path_buf(),
            plans_dir,
            patches_dir,
            writable: true,
            workspace,
        })
    }

    pub fn default_for_root(root: &Path) -> Self {
        let r = root.to_path_buf();
        Self::new(r, "plans".into(), "patches".into()).expect("Valid test workspace")
    }
}

#[allow(dead_code)]
pub fn execute_tool(
    root: &Path,
    name: &str,
    arguments: Option<&serde_json::Value>,
) -> ToolCallResult {
    match ServerConfig::new(root.to_path_buf(), "plans".into(), "patches".into()) {
        Ok(config) => execute_tool_with_config(&config, name, arguments),
        Err(error) => ToolCallResult::error(error),
    }
}

pub fn execute_tool_with_config(
    config: &ServerConfig,
    name: &str,
    arguments: Option<&serde_json::Value>,
) -> ToolCallResult {
    let empty_map = json!({});
    let args = arguments.unwrap_or(&empty_map);
    if !config.writable
        && !matches!(
            name,
            "read_file"
                | "list_directory"
                | "search_files"
                | "validate_patch"
                | "workspace_info"
                | "file_info"
        )
    {
        return ToolCallResult::error(
            "Writes require an explicit --root, WORKSPACE_ROOT, or MCP_WORKSPACE_ROOT",
        );
    }

    match name {
        "read_file" => tool_read_file(config, args),
        "list_directory" => tool_list_directory(config, args),
        "search_files" => tool_search_files(config, args),
        "write_plan_file" => tool_write_plan_file(config, args),
        "write_patch_file" => tool_write_patch_file(config, args),
        "validate_patch" => tool_validate_patch(config, args),
        _ => workspace_tools::execute(config, name, args),
    }
}

fn normalize_scope(
    workspace: &Workspace,
    input_root: &Path,
    scope: &Path,
) -> Result<std::path::PathBuf, String> {
    let scope = scope.strip_prefix(input_root).unwrap_or(scope);
    let relative = workspace.scope(scope)?;
    Ok(workspace.root().join(relative))
}

#[cfg(test)]
mod tests;
