use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

struct Client {
    child: Child,
    stdout: BufReader<std::process::ChildStdout>,
    _stderr: BufReader<std::process::ChildStderr>,
    port: u16,
    _storage: Option<tempfile::TempDir>,
}

impl Client {
    fn start(root: &std::path::Path) -> Self {
        let storage = tempfile::tempdir().unwrap();
        let mut client = Self::start_db(root, &storage.path().join("observer.sqlite3"));
        client._storage = Some(storage);
        client
    }

    fn start_db(root: &std::path::Path, database: &std::path::Path) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_file-system-mcp"))
            .args([
                "--root",
                root.to_str().unwrap(),
                "--dashboard",
                "--dashboard-port",
                "0",
                "--dashboard-db",
                database.to_str().unwrap(),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut stderr = BufReader::new(child.stderr.take().unwrap());
        let port = announced_port(&mut stderr);
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdout,
            _stderr: stderr,
            port,
            _storage: None,
        }
    }

    fn stop(&mut self) {
        self.child.stdin.take();
        assert!(self.child.wait().unwrap().success());
    }

    fn call(&mut self, params: Value) -> Value {
        let request = json!({"jsonrpc":"2.0","id":"private-request-id","method":"tools/call","params":params});
        writeln!(self.child.stdin.as_mut().unwrap(), "{request}").unwrap();
        self.child.stdin.as_mut().unwrap().flush().unwrap();
        let mut response = String::new();
        self.stdout.read_line(&mut response).unwrap();
        let response: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["id"], "private-request-id");
        response
    }

    fn snapshot(&self) -> Value {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(
            stream,
            "GET /api/snapshot HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200"));
        serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn announced_port(reader: &mut impl BufRead) -> u16 {
    let mut line = String::new();
    while reader.read_line(&mut line).unwrap() != 0 {
        if let Some((_, port)) = line
            .trim()
            .split_once("Observer dashboard: http://127.0.0.1:")
        {
            return port.parse().unwrap();
        }
        line.clear();
    }
    panic!("dashboard failed to announce its port");
}

#[test]
fn actual_tool_calls_are_observed_without_capturing_private_payloads() {
    let temporary = tempfile::tempdir().unwrap();
    fs::write(
        temporary.path().join("private-filename.txt"),
        "secret-file-content",
    )
    .unwrap();
    let mut client = Client::start(temporary.path());
    assert_eq!(client.snapshot()["summary"]["total_calls"], 0);
    client.call(json!({"name":"read_file","arguments":{"path":"private-filename.txt"}}));
    client.call(json!({"name":"write_file","arguments":{"path":"new.txt","content":"secret-write-content"}}));
    client.call(json!({"name":"read_file","arguments":{"path":"missing-private-file.txt"}}));
    client.call(
        json!({"name":"untrusted-secret-tool-name","arguments":{"secret":"secret-argument"}}),
    );
    client.call(json!({"name":42,"arguments":{"secret":"secret-invalid-param"}}));
    let snapshot = client.snapshot();
    assert_eq!(snapshot["summary"]["total_calls"], 5);
    assert_eq!(snapshot["summary"]["successes"], 2);
    assert_eq!(snapshot["summary"]["errors"], 3);
    assert_eq!(snapshot["summary"]["active_calls"], 0);
    assert_eq!(snapshot["recent_calls"][0]["outcome"], "protocol_error");
    assert_eq!(snapshot["recent_calls"][1]["tool"], "unknown_tool");
    let encoded = snapshot.to_string();
    for private in [
        "private-request-id",
        "private-filename",
        "secret-file-content",
        "secret-write-content",
        "missing-private-file",
        "untrusted-secret-tool-name",
        "secret-argument",
        "secret-invalid-param",
        temporary.path().to_str().unwrap(),
    ] {
        assert!(!encoded.contains(private), "observer retained private data");
    }
}

#[test]
fn occupied_dashboard_port_fails_with_an_actionable_error() {
    let temporary = tempfile::tempdir().unwrap();
    let storage = tempfile::tempdir().unwrap();
    let database = storage.path().join("observer.sqlite3");
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let output = Command::new(env!("CARGO_BIN_EXE_file-system-mcp"))
        .args([
            "--root",
            temporary.path().to_str().unwrap(),
            "--dashboard",
            "--dashboard-port",
            &port,
            "--dashboard-db",
            database.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Could not start observer dashboard"));
    assert!(stderr.contains("--dashboard-port"));
    assert!(output.stdout.is_empty());
}

#[test]
fn closing_stdio_stops_the_dashboard_and_server() {
    let temporary = tempfile::tempdir().unwrap();
    let mut client = Client::start(temporary.path());
    client.child.stdin.take();
    assert!(client.child.wait().unwrap().success());
    assert!(TcpStream::connect(("127.0.0.1", client.port)).is_err());
}

#[test]
fn sqlite_history_survives_server_restart_without_payloads_or_phantom_active_calls() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let database = temporary.path().join("volume/observer.sqlite3");
    let mut first = Client::start_db(&root, &database);
    first.call(json!({"name":"write_file","arguments":{"path":"private-file.txt","content":"private-persisted-content"}}));
    first.call(json!({"name":"read_file","arguments":{"path":"missing-private.txt"}}));
    let before = first.snapshot();
    assert_eq!(before["storage"]["kind"], "sqlite");
    assert_eq!(before["summary"]["total_calls"], 2);
    first.stop();
    let mut second = Client::start_db(&root, &database);
    let restored = second.snapshot();
    assert_eq!(restored["summary"], before["summary"]);
    assert_eq!(restored["recent_calls"], before["recent_calls"]);
    assert_eq!(restored["summary"]["active_calls"], 0);
    second.call(json!({"name":"file_info","arguments":{"path":"private-file.txt"}}));
    let next = second.snapshot();
    assert_eq!(next["summary"]["total_calls"], 3);
    assert_eq!(next["summary"]["successes"], 2);
    assert_eq!(next["summary"]["errors"], 1);
    assert_eq!(next["recent_calls"][0]["sequence"], 3);
    second.stop();
    let connection = rusqlite::Connection::open(&database).unwrap();
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM completed_calls", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 3);
    for private in [
        "private-persisted-content",
        "private-file.txt",
        "missing-private.txt",
        "private-request-id",
        root.to_str().unwrap(),
        database.to_str().unwrap(),
    ] {
        assert!(!next.to_string().contains(private));
        assert!(!String::from_utf8_lossy(&fs::read(&database).unwrap()).contains(private));
    }
}

#[test]
fn database_write_failure_does_not_change_a_successful_tool_result() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let database = temporary.path().join("observer.sqlite3");
    let mut client = Client::start_db(&root, &database);
    let mut connection = rusqlite::Connection::open(&database).unwrap();
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    let response = client.call(json!({"name":"write_file","arguments":{"path":"result.txt","content":"successful-file-content"}}));
    assert_eq!(response["result"]["isError"], Value::Null);
    assert_eq!(
        fs::read_to_string(root.join("result.txt")).unwrap(),
        "successful-file-content"
    );
    let snapshot = client.snapshot();
    assert_eq!(snapshot["summary"]["successes"], 1);
    assert_eq!(snapshot["storage"]["write_failures"], 1);
    assert_eq!(snapshot["storage"]["healthy"], false);
    transaction.rollback().unwrap();
    client.call(json!({"name":"file_info","arguments":{"path":"result.txt"}}));
    client.stop();
    let reopened = Client::start_db(&root, &database);
    assert_eq!(reopened.snapshot()["summary"]["total_calls"], 1);
    assert_eq!(reopened.snapshot()["recent_calls"][0]["sequence"], 2);
}

#[test]
fn sqlite_volume_cannot_be_created_inside_the_exposed_workspace() {
    let temporary = tempfile::tempdir().unwrap();
    let database = temporary.path().join("private-data/observer.sqlite3");
    let output = Command::new(env!("CARGO_BIN_EXE_file-system-mcp"))
        .args([
            "--root",
            temporary.path().to_str().unwrap(),
            "--dashboard",
            "--dashboard-port",
            "0",
            "--dashboard-db",
            database.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside the workspace"));
    assert!(!database.parent().unwrap().exists());
    assert!(output.stdout.is_empty());
}

#[test]
fn a_symlinked_database_parent_cannot_bypass_workspace_exclusion() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("workspace");
    fs::create_dir(&root).unwrap();
    let alias = temporary.path().join("alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let database = alias.join("observer.sqlite3");
    let output = Command::new(env!("CARGO_BIN_EXE_file-system-mcp"))
        .args([
            "--root",
            root.to_str().unwrap(),
            "--dashboard",
            "--dashboard-port",
            "0",
            "--dashboard-db",
            database.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside the workspace"));
    assert!(!database.exists());
}

#[test]
fn filesystem_spelling_aliases_cannot_place_sqlite_inside_the_workspace() {
    use std::os::unix::fs::MetadataExt;
    for (stored, alias) in [("workspace", "WORKSPACE"), ("caf\u{e9}", "cafe\u{301}")] {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join(stored);
        fs::create_dir(&root).unwrap();
        let aliased_root = temporary.path().join(alias);
        let Ok(aliased_metadata) = fs::metadata(&aliased_root) else {
            continue;
        };
        if aliased_metadata.ino() != fs::metadata(&root).unwrap().ino() {
            continue;
        }
        let database = aliased_root.join("new-volume/observer.sqlite3");
        let output = Command::new(env!("CARGO_BIN_EXE_file-system-mcp"))
            .args([
                "--root",
                root.to_str().unwrap(),
                "--dashboard",
                "--dashboard-port",
                "0",
                "--dashboard-db",
                database.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "filesystem alias allowed SQLite inside the workspace"
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("outside the workspace"));
        assert!(!database.parent().unwrap().exists());
    }
}
