mod instrumentation;
mod observer;
mod observer_config;
mod protocol;
mod runtime_paths;
mod security;
mod tools;
mod tunnel;
mod unified_patch;
mod workspace;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
compile_error!("Workspace filesystem tools currently support macOS and Linux");

use clap::Parser;
use protocol::{
    CallToolParams, InitializeResult, JsonRpcRequest, JsonRpcResponse, ServerCapabilities,
    ServerInfo,
};
use serde_json::json;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "file-system-mcp")]
#[command(version = env!("CARGO_PKG_VERSION"))]
#[command(about = "High-performance, secure MCP file system server for multi-repo workspaces")]
#[command(args_conflicts_with_subcommands = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<tunnel::Mode>,

    /// Root directory to expose. Defaults to workspace root (detects parent with AGENTS.md or current dir)
    #[arg(short, long)]
    pub root: Option<PathBuf>,

    /// Optional plans directory or folder name (defaults to 'plans' relative to root)
    #[arg(long = "plans-dir")]
    pub plans_dir: Option<PathBuf>,

    /// Optional patches directory or folder name (defaults to 'patches' relative to root)
    #[arg(long = "patches-dir")]
    pub patches_dir: Option<PathBuf>,

    /// Run the local MCP usage observer dashboard
    #[arg(long)]
    pub dashboard: bool,

    /// Observer dashboard port (defaults to 9411; 0 selects a free port)
    #[arg(long, requires = "dashboard")]
    pub dashboard_port: Option<u16>,

    /// Observer SQLite file (defaults to project .data/observer.sqlite3)
    #[arg(long, requires = "dashboard")]
    pub dashboard_db: Option<PathBuf>,

    /// Observer connection label; reported per-call tags take precedence
    #[arg(long, requires = "dashboard", value_name = "SOURCE")]
    pub observer_source: Option<String>,
}

pub fn determine_root_dir(cli_root: Option<PathBuf>) -> io::Result<PathBuf> {
    if let Some(root) = configured_root(cli_root) {
        return security::expand_tilde(&root).canonicalize();
    }
    discover_root()
}

fn configured_root(cli_root: Option<PathBuf>) -> Option<PathBuf> {
    cli_root
        .filter(|path| !path.to_string_lossy().trim().is_empty())
        .or_else(|| {
            ["WORKSPACE_ROOT", "MCP_WORKSPACE_ROOT"]
                .iter()
                .find_map(|name| {
                    std::env::var(name)
                        .ok()
                        .map(|value| value.trim().to_string())
                        .filter(|value| !value.is_empty())
                        .map(PathBuf::from)
                })
        })
}

fn discover_root() -> io::Result<PathBuf> {
    let current = std::env::current_dir()?;

    // If current directory is named "file-system-mcp", default to parent directory ("..")
    if let Some(name) = current.file_name()
        && name == "file-system-mcp"
        && let Some(parent) = current.parent()
    {
        return parent.canonicalize();
    }

    // If parent contains AGENTS.md (e.g. running from a subfolder), use parent
    if let Some(parent) = current.parent()
        && parent.join("AGENTS.md").is_file()
    {
        return parent.canonicalize();
    }

    // If current dir contains AGENTS.md, it's the root
    if current.join("AGENTS.md").is_file() {
        return current.canonicalize();
    }

    // Default to parent directory if it exists, otherwise current
    if let Some(parent) = current.parent()
        && let Ok(canon) = parent.canonicalize()
    {
        return Ok(canon);
    }

    // Default to current directory
    current.canonicalize()
}

pub fn create_server_config(cli: Cli) -> io::Result<tools::ServerConfig> {
    let explicit_root = configured_root(cli.root);
    let writable = explicit_root.is_some();
    let root = determine_root_dir(explicit_root)?;
    let plans_dir = security::resolve_scoped_dir(
        cli.plans_dir.as_deref(),
        &["PLANS_DIR", "MCP_PLANS_DIR"],
        "plans",
        &root,
    );
    let patches_dir = security::resolve_scoped_dir(
        cli.patches_dir.as_deref(),
        &["PATCHES_DIR", "MCP_PATCHES_DIR"],
        "patches",
        &root,
    );
    let mut config = tools::ServerConfig::new(root, plans_dir, patches_dir)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    config.writable = writable;
    Ok(config)
}

fn process_request(config: &tools::ServerConfig, req: JsonRpcRequest) -> Option<JsonRpcResponse> {
    let id = req.id;

    match req.method.as_str() {
        "initialize" => {
            let result = InitializeResult {
                protocol_version: "2024-11-05".to_string(),
                capabilities: ServerCapabilities { tools: json!({}) },
                server_info: ServerInfo {
                    name: "file-system-mcp",
                    version: env!("CARGO_PKG_VERSION"),
                },
            };
            Some(JsonRpcResponse::success(
                id,
                serde_json::to_value(result).unwrap(),
            ))
        }
        "notifications/initialized" => {
            // Notifications do not receive responses
            None
        }
        "ping" => Some(JsonRpcResponse::success(id, json!({}))),
        "tools/list" => {
            let tools = tools::get_available_tools();
            Some(JsonRpcResponse::success(id, json!({ "tools": tools })))
        }
        "tools/call" => {
            let params: CallToolParams = match req.params {
                Some(p) => match serde_json::from_value(p) {
                    Ok(parsed) => parsed,
                    Err(e) => {
                        return Some(JsonRpcResponse::error(
                            id,
                            -32602,
                            format!("Invalid params for tools/call: {}", e),
                        ));
                    }
                },
                None => {
                    return Some(JsonRpcResponse::error(
                        id,
                        -32602,
                        "Missing params for tools/call",
                    ));
                }
            };

            let tool_result =
                tools::execute_tool_with_config(config, &params.name, params.arguments.as_ref());
            Some(JsonRpcResponse::success(
                id,
                serde_json::to_value(tool_result).unwrap(),
            ))
        }
        other => {
            if id.is_some() {
                Some(JsonRpcResponse::error(
                    id,
                    -32601,
                    format!("Method not found: '{}'", other),
                ))
            } else {
                // Ignore unknown notifications
                None
            }
        }
    }
}

fn main() -> io::Result<()> {
    let mut cli = Cli::parse();
    if let Some(mode) = cli.command.take() {
        return tunnel::launch(mode);
    }
    let observer_context = if cli.dashboard {
        observer_config::origin_context(cli.observer_source.as_deref())?
    } else {
        observer::origin::Context::default()
    };
    let dashboard_options = (
        cli.dashboard,
        cli.dashboard_port.unwrap_or(9411),
        cli.dashboard_db.clone(),
    );
    let config = create_server_config(cli)?;
    let dashboard = start_dashboard(dashboard_options, &config.root)?;
    let observer = dashboard.as_ref().map(observer::Dashboard::observer);

    eprintln!(
        "[file-system-mcp] Started. Workspace root: {}\n[file-system-mcp] Plans directory: {}\n[file-system-mcp] Patches directory: {}",
        config.root.display(),
        config.plans_dir.display(),
        config.patches_dir.display(),
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line_res in stdin.lock().lines() {
        let line = match line_res {
            Ok(l) => l,
            Err(e) => {
                eprintln!("[file-system-mcp] Error reading stdin: {}", e);
                break;
            }
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let response = match serde_json::from_str::<JsonRpcRequest>(trimmed) {
            Ok(req) => instrumentation::process(
                &config,
                observer.as_deref(),
                req,
                trimmed.len() as u64,
                observer_context,
            ),
            Err(e) => Some(JsonRpcResponse::error(
                None,
                -32700,
                format!("Parse error: {}", e),
            )),
        };

        if let Some(resp) = response
            && let Ok(json_str) = serde_json::to_string(&resp)
        {
            writeln!(stdout, "{}", json_str)?;
            stdout.flush()?;
        }
    }

    eprintln!("[file-system-mcp] Stdio stream closed. Exiting.");
    Ok(())
}

fn start_dashboard(
    (enabled, port, database): (bool, u16, Option<PathBuf>),
    workspace: &std::path::Path,
) -> io::Result<Option<observer::Dashboard>> {
    if !enabled {
        return Ok(None);
    }
    let database = observer_config::database_path(database, workspace)?;
    let dashboard = observer::Dashboard::start_persistent(port, &database).map_err(|error| {
        io::Error::new(error.kind(), format!("Could not start observer dashboard: {error}. Check --dashboard-db / MCP_OBSERVER_DB and --dashboard-port, or stop the existing server"))
    })?;
    eprintln!("[file-system-mcp] Observer dashboard: {}", dashboard.url());
    eprintln!(
        "[file-system-mcp] Observer SQLite database: {}",
        database.display()
    );
    Ok(Some(dashboard))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_initialize() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = tools::ServerConfig::default_for_root(temp_dir.path());
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(1)),
            method: "initialize".to_string(),
            params: Some(json!({"protocolVersion": "2024-11-05"})),
        };

        let resp = process_request(&config, req).expect("Must return response");
        assert_eq!(resp.id, Some(json!(1)));
        assert!(resp.result.is_some());
        let val = resp.result.unwrap();
        assert_eq!(val["protocolVersion"], "2024-11-05");
        assert_eq!(val["serverInfo"]["name"], "file-system-mcp");
        assert_eq!(val["serverInfo"]["version"], env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn test_process_ping() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = tools::ServerConfig::default_for_root(temp_dir.path());
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!("abc")),
            method: "ping".to_string(),
            params: None,
        };

        let resp = process_request(&config, req).expect("Must return response");
        assert_eq!(resp.id, Some(json!("abc")));
    }

    #[test]
    fn test_process_tools_list() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = tools::ServerConfig::default_for_root(temp_dir.path());
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(2)),
            method: "tools/list".to_string(),
            params: None,
        };

        let resp = process_request(&config, req).expect("Must return response");
        let val = resp.result.unwrap();
        assert!(val["tools"].as_array().unwrap().len() >= 3);
    }

    #[test]
    fn test_process_unknown_method() {
        let temp_dir = tempfile::tempdir().unwrap();
        let config = tools::ServerConfig::default_for_root(temp_dir.path());
        let req = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            id: Some(json!(3)),
            method: "non_existent_method".to_string(),
            params: None,
        };

        let resp = process_request(&config, req).expect("Must return response");
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, -32601);
    }

    #[test]
    fn test_determine_root_dir_cli_and_relative() {
        let temp_dir = tempfile::tempdir().unwrap();
        let canon = temp_dir.path().canonicalize().unwrap();

        let resolved = determine_root_dir(Some(temp_dir.path().to_path_buf())).unwrap();
        assert_eq!(resolved, canon);

        // Test relative path resolution for current dir
        let curr_resolved = determine_root_dir(Some(PathBuf::from("."))).unwrap();
        assert_eq!(
            curr_resolved,
            std::env::current_dir().unwrap().canonicalize().unwrap()
        );
    }

    #[test]
    fn test_create_server_config() {
        let temp_dir = tempfile::tempdir().unwrap();
        let canon = temp_dir.path().canonicalize().unwrap();

        let cli = Cli {
            command: None,
            dashboard: false,
            dashboard_port: None,
            dashboard_db: None,
            observer_source: None,
            root: Some(temp_dir.path().to_path_buf()),
            plans_dir: Some(PathBuf::from("my_plans")),
            patches_dir: Some(PathBuf::from("my_patches")),
        };

        let config = create_server_config(cli).unwrap();
        assert_eq!(config.root, canon);
        assert_eq!(config.plans_dir, canon.join("my_plans"));
        assert_eq!(config.patches_dir, canon.join("my_patches"));
    }
}
