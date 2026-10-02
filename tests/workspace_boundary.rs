mod workspace_support;

use serde_json::json;
use std::fs;
use std::process::{Command, Stdio};
use workspace_support::{McpClient, call, failure, success};

#[test]
fn discovered_roots_allow_reads_but_reject_all_writes() {
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("file-system-mcp");
    fs::create_dir(&cwd).unwrap();
    fs::write(root.path().join("readme.txt"), "readable").unwrap();
    let mut client = McpClient::spawn_with_options(
        &[],
        &[("WORKSPACE_ROOT", ""), ("MCP_WORKSPACE_ROOT", "")],
        Some(&cwd),
    );
    let info = success(&mut client, "workspace_info", json!({}));
    assert_eq!(info["writable"], false);
    assert_ne!(
        call(&mut client, "read_file", json!({"path": "readme.txt"}))["isError"],
        true
    );
    for (name, args) in [
        ("write_file", json!({"path": "new.txt", "content": "body"})),
        (
            "write_plan_file",
            json!({"filename": "plan.md", "content": "# Plan"}),
        ),
        (
            "write_patch_file",
            json!({"filename": "diff.patch", "content": "diff"}),
        ),
        ("create_directory", json!({"path": "dir"})),
    ] {
        assert!(failure(&mut client, name, args).contains("explicit"));
    }
    assert!(!root.path().join("plans").exists());
    assert!(!root.path().join("new.txt").exists());
}

#[test]
fn startup_rejects_plan_and_patch_directories_outside_the_root() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    for option in ["--plans-dir", "--patches-dir"] {
        let output = Command::new(env!("CARGO_BIN_EXE_file-system-mcp"))
            .args([
                "--root",
                root.path().to_str().unwrap(),
                option,
                external.path().to_str().unwrap(),
            ])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "Accepted {option} outside the workspace"
        );
    }
}

#[test]
fn all_operands_reject_traversal_absolute_paths_and_workspace_root_mutation() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(root.path().join("source.txt"), "safe").unwrap();
    fs::write(external.path().join("outside.txt"), "outside").unwrap();
    let mut client = McpClient::spawn(root.path());
    for path in [
        "../outside.txt",
        "sub/../../outside.txt",
        external.path().join("outside.txt").to_str().unwrap(),
    ] {
        failure(
            &mut client,
            "write_file",
            json!({"path": path, "content": "evil", "overwrite": true}),
        );
        failure(&mut client, "file_info", json!({"path": path}));
        failure(
            &mut client,
            "move_path",
            json!({"source": "source.txt", "destination": path}),
        );
        failure(
            &mut client,
            "copy_path",
            json!({"source": path, "destination": "copied.txt"}),
        );
        failure(
            &mut client,
            "delete_path",
            json!({"path": path, "recursive": true}),
        );
    }
    for path in [".", "./", ""] {
        failure(
            &mut client,
            "delete_path",
            json!({"path": path, "recursive": true}),
        );
        failure(
            &mut client,
            "move_path",
            json!({"source": path, "destination": "moved"}),
        );
    }
    assert_eq!(
        fs::read(external.path().join("outside.txt")).unwrap(),
        b"outside"
    );
    assert_eq!(fs::read(root.path().join("source.txt")).unwrap(), b"safe");
}

#[test]
fn directory_operations_cannot_bypass_protected_paths_or_gitignore() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("repo/.git")).unwrap();
    fs::write(root.path().join("repo/.git/config"), "metadata").unwrap();
    fs::write(root.path().join("repo/code.txt"), "safe").unwrap();
    fs::write(root.path().join(".gitignore"), "ignored/\n").unwrap();
    fs::create_dir(root.path().join("ignored")).unwrap();
    let mut client = McpClient::spawn(root.path());
    for path in [
        ".env",
        "private.key",
        ".ssh/config",
        "repo/.git/config",
        "ignored/new.txt",
    ] {
        failure(
            &mut client,
            "write_file",
            json!({"path": path, "content": "evil", "overwrite": true}),
        );
    }
    for tool in ["copy_path", "move_path"] {
        failure(
            &mut client,
            tool,
            json!({"source": "repo", "destination": "other", "recursive": true}),
        );
        failure(
            &mut client,
            tool,
            json!({"source": "repo/code.txt", "destination": "repo/.git/new"}),
        );
    }
    failure(
        &mut client,
        "delete_path",
        json!({"path": "repo", "recursive": true}),
    );
    assert_eq!(
        fs::read(root.path().join("repo/.git/config")).unwrap(),
        b"metadata"
    );
    assert!(root.path().join("repo/code.txt").exists());
    assert!(!root.path().join("other").exists());
}

#[test]
#[cfg(unix)]
fn symlink_sources_destinations_and_ancestors_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("outside.txt"), "outside").unwrap();
    fs::write(root.path().join("safe.txt"), "safe").unwrap();
    std::os::unix::fs::symlink(external.path(), root.path().join("escape")).unwrap();
    std::os::unix::fs::symlink(
        external.path().join("outside.txt"),
        root.path().join("link.txt"),
    )
    .unwrap();
    let mut client = McpClient::spawn(root.path());
    for path in ["escape/outside.txt", "link.txt"] {
        failure(
            &mut client,
            "write_file",
            json!({"path": path, "content": "evil", "overwrite": true}),
        );
        failure(&mut client, "file_info", json!({"path": path}));
        failure(
            &mut client,
            "delete_path",
            json!({"path": path, "recursive": true}),
        );
        failure(
            &mut client,
            "copy_path",
            json!({"source": path, "destination": "copy.txt"}),
        );
        failure(
            &mut client,
            "move_path",
            json!({"source": "safe.txt", "destination": path, "overwrite": true}),
        );
    }
    assert_eq!(
        fs::read(external.path().join("outside.txt")).unwrap(),
        b"outside"
    );
}

#[test]
#[cfg(unix)]
fn atomic_replacement_does_not_modify_an_outside_hard_link() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("outside.txt"), "original").unwrap();
    fs::hard_link(
        external.path().join("outside.txt"),
        root.path().join("inside.txt"),
    )
    .unwrap();
    let mut client = McpClient::spawn(root.path());
    success(
        &mut client,
        "write_file",
        json!({"path": "inside.txt", "content": "replacement", "overwrite": true}),
    );
    assert_eq!(
        fs::read(external.path().join("outside.txt")).unwrap(),
        b"original"
    );
    assert_eq!(
        fs::read(root.path().join("inside.txt")).unwrap(),
        b"replacement"
    );
}
