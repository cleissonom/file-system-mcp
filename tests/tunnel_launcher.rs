use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture {
    temporary: tempfile::TempDir,
    project: PathBuf,
    binary: PathBuf,
    home: PathBuf,
    client_bin: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let project = temporary
            .path()
            .join("project with 'quotes,=and$characters");
        let release = project.join("target/release");
        fs::create_dir_all(&release).unwrap();
        fs::write(project.join("Cargo.toml"), "[package]\nname = 'fixture'\n").unwrap();
        let binary = release.join("file-system-mcp");
        fs::copy(env!("CARGO_BIN_EXE_file-system-mcp"), &binary).unwrap();
        let home = temporary.path().join("home");
        fs::create_dir_all(home.join("workspace")).unwrap();
        let client_bin = temporary.path().join("client-bin");
        fs::create_dir(&client_bin).unwrap();
        write_fake_client(&client_bin);
        Self {
            temporary,
            project,
            binary,
            home,
            client_bin,
        }
    }

    fn configure(&self, content: &str) {
        fs::write(self.project.join(".env"), content).unwrap();
    }

    fn defaults(&self) {
        self.configure("CONTROL_PLANE_API_KEY='sk-fixture-secret'\nTUNNEL_ID=tunnel_fixture\nWORKSPACE_ROOT=\"$HOME/workspace\"\n");
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.binary);
        command.arg("tunnel").env_clear();
        command
            .env("PATH", &self.client_bin)
            .env("HOME", &self.home);
        command.env("CAPTURE_ARGS", self.capture("args"));
        command.env("CAPTURE_ENV", self.capture("env"));
        command.env("CAPTURE_MCP", self.capture("mcp"));
        command.env("CAPTURE_SOURCE", self.capture("source"));
        command.env("MCP_DASHBOARD_PORT", "0");
        command.current_dir(self.temporary.path());
        command
    }

    fn capture(&self, name: &str) -> PathBuf {
        self.temporary.path().join(name)
    }

    fn captured(&self, name: &str) -> String {
        fs::read_to_string(self.capture(name)).unwrap()
    }
}

fn write_fake_client(directory: &Path) {
    let client = directory.join("tunnel-client");
    fs::write(&client, r#"#!/bin/sh
printf '%s\n' "$@" > "$CAPTURE_ARGS"
printf '%s\n' "$CONTROL_PLANE_API_KEY" "$CONTROL_PLANE_TUNNEL_ID" "$WORKSPACE_ROOT" > "$CAPTURE_ENV"
printf '%s\n' "$MCP_OBSERVER_SOURCE" "$FILE_SYSTEM_MCP_TUNNEL_LAUNCH" > "$CAPTURE_SOURCE"
while [ "$#" -gt 0 ]; do
    case "$1" in
        --mcp.command) mcp_command=$2; shift 2 ;;
        *) shift ;;
    esac
done
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"workspace_info","arguments":{}}}' | /bin/sh -c "$mcp_command" > "$CAPTURE_MCP"
exit "${FAKE_EXIT_CODE:-0}"
"#).unwrap();
    fs::set_permissions(client, fs::Permissions::from_mode(0o700)).unwrap();
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn observer_source_uses_cli_then_shell_then_file_and_marks_tunnel_context() {
    let fixture = Fixture::new();
    fixture.defaults();
    let env_file = fixture.project.join(".env");
    let mut config = fs::read_to_string(&env_file).unwrap();
    config.push_str("MCP_OBSERVER_SOURCE=chatgpt_work\n");
    fixture.configure(&config);
    assert_success(&fixture.command().output().unwrap());
    assert_eq!(fixture.captured("source"), "chatgpt_work\n1\n");
    assert_success(
        &fixture
            .command()
            .env("MCP_OBSERVER_SOURCE", "codex_cloud")
            .output()
            .unwrap(),
    );
    assert_eq!(fixture.captured("source"), "codex_cloud\n1\n");
    assert_success(
        &fixture
            .command()
            .env("MCP_OBSERVER_SOURCE", "codex_cloud")
            .args(["--observer-source", "openai_dot"])
            .output()
            .unwrap(),
    );
    assert_eq!(fixture.captured("source"), "openai_dot\n1\n");
}

#[test]
fn invalid_tunnel_source_fails_without_echoing_its_value_or_starting_client() {
    let fixture = Fixture::new();
    fixture.defaults();
    let output = fixture
        .command()
        .env("MCP_OBSERVER_SOURCE", "private-source-sentinel")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("private-source-sentinel"));
    assert!(!fixture.capture("args").exists());
}

fn workspace_info(fixture: &Fixture) -> Value {
    let response: Value = serde_json::from_str(&fixture.captured("mcp")).unwrap();
    serde_json::from_str(response["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn project_configuration_and_executable_are_found_from_any_directory() {
    let fixture = Fixture::new();
    fixture.defaults();
    fs::write(
        fixture.temporary.path().join(".env"),
        "CONTROL_PLANE_API_KEY=wrong-cwd-key\n",
    )
    .unwrap();
    let output = fixture.command().output().unwrap();
    assert_success(&output);
    let arguments = fixture.captured("args");
    assert!(arguments.contains("--mcp.command\n"));
    assert!(!arguments.contains("sk-fixture-secret"));
    assert_eq!(
        fixture.captured("env"),
        format!(
            "sk-fixture-secret\ntunnel_fixture\n{}\n",
            fixture
                .home
                .join("workspace")
                .canonicalize()
                .unwrap()
                .display()
        )
    );
    let info = workspace_info(&fixture);
    assert_eq!(info["writable"], true);
    assert_eq!(
        info["root"],
        fixture
            .home
            .join("workspace")
            .canonicalize()
            .unwrap()
            .to_str()
            .unwrap()
    );
}

#[test]
fn inherited_configuration_overrides_dotenv_values() {
    let fixture = Fixture::new();
    fixture.defaults();
    let root = fixture.home.join("override");
    fs::create_dir(&root).unwrap();
    let output = fixture
        .command()
        .env("CONTROL_PLANE_API_KEY", "sk-environment")
        .env("CONTROL_PLANE_TUNNEL_ID", "tunnel_environment")
        .env("WORKSPACE_ROOT", &root)
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        fixture.captured("env"),
        format!(
            "sk-environment\ntunnel_environment\n{}\n",
            root.canonicalize().unwrap().display()
        )
    );
}

#[test]
fn inherited_aliases_override_canonical_dotenv_names() {
    let fixture = Fixture::new();
    fixture.configure("CONTROL_PLANE_API_KEY=sk-file\nCONTROL_PLANE_TUNNEL_ID=tunnel_file\nWORKSPACE_ROOT=\"$HOME/workspace\"\n");
    let output = fixture
        .command()
        .env("OPENAI_API_KEY", "sk-alias")
        .env("TUNNEL_ID", "tunnel_alias")
        .output()
        .unwrap();
    assert_success(&output);
    assert!(
        fixture
            .captured("env")
            .starts_with("sk-alias\ntunnel_alias\n")
    );
}

#[test]
fn an_explicit_env_file_overrides_default_discovery() {
    let fixture = Fixture::new();
    fixture.defaults();
    let file = fixture.temporary.path().join("custom.env");
    fs::write(&file, "CONTROL_PLANE_API_KEY=sk-custom\nTUNNEL_ID=tunnel_custom\nWORKSPACE_ROOT=\"$HOME/workspace\"\n").unwrap();
    let output = fixture
        .command()
        .args(["--env-file", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_success(&output);
    assert!(
        fixture
            .captured("env")
            .starts_with("sk-custom\ntunnel_custom\n")
    );
}

#[test]
fn tunnel_client_exit_status_is_preserved() {
    let fixture = Fixture::new();
    fixture.defaults();
    let output = fixture
        .command()
        .env("FAKE_EXIT_CODE", "17")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(17));
}

#[test]
fn missing_configuration_fails_before_launching_the_client() {
    for (content, missing) in [
        (
            "TUNNEL_ID=tunnel_fixture\nWORKSPACE_ROOT=\"$HOME/workspace\"\n",
            "CONTROL_PLANE_API_KEY",
        ),
        (
            "CONTROL_PLANE_API_KEY=sk-fixture\nWORKSPACE_ROOT=\"$HOME/workspace\"\n",
            "TUNNEL_ID",
        ),
        (
            "CONTROL_PLANE_API_KEY=sk-fixture\nTUNNEL_ID=tunnel_fixture\n",
            "WORKSPACE_ROOT",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.configure(content);
        let output = fixture.command().output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(missing));
        assert!(!fixture.capture("args").exists());
    }
}

#[test]
fn invalid_configuration_does_not_expose_secret_values() {
    let fixture = Fixture::new();
    fixture.configure("CONTROL_PLANE_API_KEY='sk-secret-must-not-be-printed\n");
    let output = fixture.command().output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(".env"));
    assert!(!stderr.contains("sk-secret-must-not-be-printed"));
    assert!(!fixture.capture("args").exists());
}

#[test]
fn invalid_workspace_root_fails_before_launching_the_client() {
    let fixture = Fixture::new();
    fixture.defaults();
    let file = fixture.temporary.path().join("regular-file");
    fs::write(&file, "not a directory").unwrap();
    let output = fixture
        .command()
        .env("WORKSPACE_ROOT", &file)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("WORKSPACE_ROOT"));
    assert!(!fixture.capture("args").exists());
}

#[test]
fn tunnel_launches_the_dashboard_and_removes_the_browser_admin_listener() {
    let fixture = Fixture::new();
    fixture.defaults();
    let output = fixture.command().output().unwrap();
    assert_success(&output);
    let args = fixture.captured("args");
    assert!(args.contains("--dashboard --dashboard-port 0"));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Observer dashboard: http://127.0.0.1:")
    );
    assert!(args.contains("--health.unix-socket\n"));
    assert!(args.contains("--open-web-ui=false"));
    assert!(!args.contains("--health.listen-addr"));
    let socket = args
        .lines()
        .skip_while(|line| *line != "--health.unix-socket")
        .nth(1)
        .unwrap();
    let metadata = fs::symlink_metadata(Path::new(socket).parent().unwrap()).unwrap();
    assert!(metadata.is_dir());
    assert_eq!(metadata.permissions().mode() & 0o777, 0o700);
}

#[test]
fn dashboard_can_be_disabled_for_headless_tunnel_use() {
    let fixture = Fixture::new();
    fixture.defaults();
    let output = fixture.command().arg("--no-dashboard").output().unwrap();
    assert_success(&output);
    assert!(!fixture.captured("args").contains("--dashboard"));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("Observer dashboard:"));
}

#[test]
fn invalid_dashboard_port_is_rejected_before_client_launch() {
    let fixture = Fixture::new();
    fixture.defaults();
    let output = fixture
        .command()
        .env("MCP_DASHBOARD_PORT", "invalid")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("MCP_DASHBOARD_PORT"));
    assert!(!fixture.capture("args").exists());
}

#[test]
fn tunnel_default_sqlite_volume_is_project_relative_and_restores_history() {
    let fixture = Fixture::new();
    fixture.defaults();
    assert_success(&fixture.command().output().unwrap());
    assert_success(&fixture.command().output().unwrap());
    let database = fixture.project.join(".data/observer.sqlite3");
    let connection = rusqlite::Connection::open(&database).unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM completed_calls", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 2);
    assert!(!fixture.temporary.path().join(".data").exists());
}

#[test]
fn tunnel_database_configuration_obeys_file_environment_and_cli_precedence() {
    let fixture = Fixture::new();
    fixture.configure("CONTROL_PLANE_API_KEY=sk-fixture\nTUNNEL_ID=tunnel_fixture\nWORKSPACE_ROOT=\"$HOME/workspace\"\nMCP_OBSERVER_DB=volume/from-file.sqlite3\n");
    assert_success(&fixture.command().output().unwrap());
    assert!(fixture.project.join("volume/from-file.sqlite3").is_file());
    assert_success(
        &fixture
            .command()
            .env("MCP_OBSERVER_DB", "volume/from-shell.sqlite3")
            .output()
            .unwrap(),
    );
    assert!(fixture.project.join("volume/from-shell.sqlite3").is_file());
    let cli_path = fixture
        .temporary
        .path()
        .join("cli 'quoted/observer.sqlite3");
    assert_success(
        &fixture
            .command()
            .env("MCP_OBSERVER_DB", "volume/unselected.sqlite3")
            .arg("--dashboard-db")
            .arg(&cli_path)
            .output()
            .unwrap(),
    );
    assert!(cli_path.is_file());
    assert!(!fixture.project.join("volume/unselected.sqlite3").exists());
}
