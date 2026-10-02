use std::collections::HashSet;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn local_configuration_and_database_files_are_ignored() {
    let paths = [
        ".env",
        ".env.local",
        ".env.production",
        "config/.env.production",
        ".envrc",
        "custom.env",
        "config/config.env",
        ".data/observer.sqlite3",
        "private/observer.sqlite3",
        "private/observer.sqlite3-wal",
        "private/observer.sqlite3-shm",
        "private/observer.sqlite3-journal",
        "private/observer.sqlite3.lock",
        "private/observer.sqlite",
        "private/observer.db",
        "private/observer.db-wal",
        "private/client.pem",
        "private/client.key",
        "private/client.p12",
        "private/client.pfx",
        "target/release/file-system-mcp",
    ];
    let ignored = ignored_paths(&paths);
    for path in paths {
        assert!(
            ignored.contains(path),
            "private artifact is publishable: {path}"
        );
    }
}

#[test]
fn examples_and_migration_source_remain_publishable() {
    let paths = [
        ".env.example",
        "README.md",
        "src/main.rs",
        "src/observer/storage/migrations/0001_calls.sql",
    ];
    assert!(ignored_paths(&paths).is_empty());
}

fn ignored_paths(paths: &[&str]) -> HashSet<String> {
    let repository = publication_fixture();
    let mut child = git()
        .args(["check-ignore", "--no-index", "--stdin"])
        .current_dir(repository.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Git is required for publication policy checks");
    writeln!(child.stdin.take().unwrap(), "{}", paths.join("\n")).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(matches!(output.status.code(), Some(0 | 1)));
    String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn publication_fixture() -> tempfile::TempDir {
    let repository = tempfile::tempdir().unwrap();
    assert!(
        git()
            .args(["init", "--quiet"])
            .current_dir(repository.path())
            .status()
            .unwrap()
            .success()
    );
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".gitignore"),
        repository.path().join(".gitignore"),
    )
    .unwrap();
    repository
}

fn git() -> Command {
    let mut command = Command::new("git");
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null");
    command
}
