use crate::observer::origin::Source;
use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "tunnel/health.rs"]
mod health;

#[derive(clap::Subcommand, Debug, Clone)]
pub enum Mode {
    /// Load project .env and serve this executable through tunnel-client
    Tunnel {
        /// Configuration file; defaults to project .env, or .env beside a relocated binary
        #[arg(long, value_name = "PATH")]
        env_file: Option<PathBuf>,

        /// Dashboard port (overrides MCP_DASHBOARD_PORT; defaults to 9411)
        #[arg(long)]
        dashboard_port: Option<u16>,

        /// Observer SQLite file (overrides MCP_OBSERVER_DB)
        #[arg(long)]
        dashboard_db: Option<PathBuf>,

        /// Observer connection label (overrides MCP_OBSERVER_SOURCE)
        #[arg(long, value_name = "SOURCE")]
        observer_source: Option<String>,

        /// Run without the MCP observer dashboard
        #[arg(long, conflicts_with_all = ["dashboard_port", "dashboard_db", "observer_source"])]
        no_dashboard: bool,
    },
}

pub fn launch(mode: Mode) -> io::Result<()> {
    let Mode::Tunnel {
        env_file,
        dashboard_port,
        dashboard_db,
        observer_source,
        no_dashboard,
    } = mode;
    let executable = std::env::current_exe()?;
    let file = env_file.unwrap_or_else(|| default_env_file(&executable));
    let settings = Settings::load(&file)?;
    let port = if no_dashboard {
        None
    } else {
        Some(settings.dashboard_port(dashboard_port)?)
    };
    let database = if no_dashboard {
        None
    } else {
        settings.database_path(dashboard_db, &file)?
    };
    let source = if no_dashboard {
        None
    } else {
        Some(settings.observer_source(observer_source.as_deref())?)
    };
    let mut command = settings.command(&executable, &file, port, database, source)?;
    let error = command.exec();
    Err(io::Error::new(
        error.kind(),
        format!("Could not launch tunnel-client from PATH: {error}"),
    ))
}

fn default_env_file(executable: &Path) -> PathBuf {
    crate::runtime_paths::project_directory(executable).join(".env")
}

struct Settings {
    file: HashMap<String, String>,
    inherited: HashMap<OsString, OsString>,
}

impl Settings {
    fn load(path: &Path) -> io::Result<Self> {
        let entries =
            dotenvy::from_path_iter(path).map_err(|error| configuration_error(path, error))?;
        let mut file = HashMap::new();
        for entry in entries {
            let (name, value) = entry.map_err(|error| configuration_error(path, error))?;
            file.entry(name).or_insert(value);
        }
        Ok(Self {
            file,
            inherited: std::env::vars_os().collect(),
        })
    }

    fn value(&self, names: &[&str]) -> io::Result<String> {
        for name in names {
            if let Some(value) = self.inherited.get(OsStr::new(name)) {
                return required_value(value.to_str(), names[0]);
            }
        }
        for name in names {
            if let Some(value) = self.file.get(*name) {
                return required_value(Some(value), names[0]);
            }
        }
        required_value(None, names[0])
    }

    fn dashboard_port(&self, cli_port: Option<u16>) -> io::Result<u16> {
        if let Some(port) = cli_port {
            return Ok(port);
        }
        let name = "MCP_DASHBOARD_PORT";
        if !self.inherited.contains_key(OsStr::new(name)) && !self.file.contains_key(name) {
            return Ok(9411);
        }
        self.value(&[name])?.parse().map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "MCP_DASHBOARD_PORT must be an integer from 0 to 65535",
            )
        })
    }

    fn database_path(&self, cli: Option<PathBuf>, file: &Path) -> io::Result<Option<PathBuf>> {
        let (path, base) = if let Some(path) = cli {
            (path, std::env::current_dir()?)
        } else {
            let name = "MCP_OBSERVER_DB";
            if !self.inherited.contains_key(OsStr::new(name)) && !self.file.contains_key(name) {
                return Ok(None);
            }
            (
                PathBuf::from(self.value(&[name])?),
                file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            )
        };
        let path = crate::security::expand_tilde(&path);
        Ok(Some(if path.is_absolute() {
            path
        } else {
            base.join(path)
        }))
    }

    fn observer_source(&self, cli: Option<&str>) -> io::Result<Source> {
        if let Some(value) = cli {
            return crate::observer_config::parse_source(value);
        }
        let name = "MCP_OBSERVER_SOURCE";
        if !self.inherited.contains_key(OsStr::new(name)) && !self.file.contains_key(name) {
            return Ok(Source::Unknown);
        }
        crate::observer_config::parse_source(&self.value(&[name])?)
    }

    fn command(
        &self,
        executable: &Path,
        file: &Path,
        dashboard_port: Option<u16>,
        database: Option<PathBuf>,
        source: Option<Source>,
    ) -> io::Result<Command> {
        let api_key = self.value(&["CONTROL_PLANE_API_KEY", "OPENAI_API_KEY"])?;
        let tunnel_id = self.value(&["CONTROL_PLANE_TUNNEL_ID", "TUNNEL_ID"])?;
        let root = workspace_root(
            &self.value(&["WORKSPACE_ROOT", "MCP_WORKSPACE_ROOT"])?,
            file,
        )?;
        let mut command = Command::new("tunnel-client");
        command.args([
            "run",
            "--control-plane.api-key",
            "env:CONTROL_PLANE_API_KEY",
        ]);
        command.args([
            "--control-plane.tunnel-id",
            &tunnel_id,
            "--mcp.command",
            &mcp_command(executable, dashboard_port)?,
        ]);
        command
            .arg("--health.unix-socket")
            .arg(health::socket_path()?);
        command.arg("--open-web-ui=false");
        command.env_clear().envs(&self.file).envs(&self.inherited);
        command.env("CONTROL_PLANE_API_KEY", api_key);
        command.env("CONTROL_PLANE_TUNNEL_ID", tunnel_id);
        command.env("WORKSPACE_ROOT", root);
        command.env("FILE_SYSTEM_MCP_TUNNEL_LAUNCH", "1");
        if let Some(source) = source {
            command.env("MCP_OBSERVER_SOURCE", source.as_str());
        }
        if let Some(path) = database {
            command.env("MCP_OBSERVER_DB", path);
        }
        Ok(command)
    }
}

fn mcp_command(executable: &Path, dashboard_port: Option<u16>) -> io::Result<String> {
    let command = quoted_executable(executable)?;
    Ok(match dashboard_port {
        Some(port) => format!("{command} --dashboard --dashboard-port {port}"),
        None => command,
    })
}

fn required_value(value: Option<&str>, name: &str) -> io::Result<String> {
    match value {
        Some(value) if !value.trim().is_empty() && !value.contains('\0') => {
            Ok(value.trim().to_owned())
        }
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Missing or invalid {name}; configure it in .env or the environment"),
        )),
    }
}

fn workspace_root(value: &str, file: &Path) -> io::Result<PathBuf> {
    let path = crate::security::expand_tilde(Path::new(value));
    let path = if path.is_absolute() {
        path
    } else {
        file.parent().unwrap_or(Path::new(".")).join(path)
    };
    let root = path.canonicalize().map_err(|error| {
        io::Error::new(
            error.kind(),
            "WORKSPACE_ROOT must name an existing directory",
        )
    })?;
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "WORKSPACE_ROOT must name an existing directory",
        ));
    }
    Ok(root)
}

fn quoted_executable(executable: &Path) -> io::Result<String> {
    let path = executable.to_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "MCP executable path must be valid UTF-8",
        )
    })?;
    // tunnel-client's bare command tokenizer accepts POSIX quoting without invoking a shell.
    Ok(format!("'{}'", path.replace('\'', "'\\''")))
}

fn configuration_error(path: &Path, error: dotenvy::Error) -> io::Error {
    // dotenv errors can contain the input line, including an API key.
    match error {
        dotenvy::Error::Io(error) => io::Error::new(
            error.kind(),
            format!(
                "Could not read configuration file {}: {error}",
                path.display()
            ),
        ),
        _ => io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Could not parse configuration file {}: invalid dotenv syntax",
                path.display()
            ),
        ),
    }
}
