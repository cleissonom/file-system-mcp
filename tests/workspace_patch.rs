mod workspace_support;

use serde_json::json;
use std::fs;
use workspace_support::{McpClient, failure, success};

#[test]
fn a_validated_patch_changes_multiple_files_and_can_create_and_delete_files() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("first.txt"), "before\n").unwrap();
    fs::write(root.path().join("removed.txt"), "remove\n").unwrap();
    let patch = concat!(
        "--- a/first.txt\n+++ b/first.txt\n@@ -1 +1 @@\n-before\n+after\n",
        "--- /dev/null\n+++ b/created.txt\n@@ -0,0 +1 @@\n+created\n",
        "--- a/removed.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-remove\n"
    );
    let mut client = McpClient::spawn(root.path());
    let result = success(&mut client, "apply_patch", json!({"patch_content": patch}));
    assert_eq!(result["changed_paths"].as_array().unwrap().len(), 3);
    assert_eq!(fs::read(root.path().join("first.txt")).unwrap(), b"after\n");
    assert_eq!(
        fs::read(root.path().join("created.txt")).unwrap(),
        b"created\n"
    );
    assert!(!root.path().join("removed.txt").exists());
}

#[test]
fn patch_validation_failure_preserves_every_target_file() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("first.txt"), "before\n").unwrap();
    fs::write(root.path().join("second.txt"), "actual\n").unwrap();
    let patch = concat!(
        "--- a/first.txt\n+++ b/first.txt\n@@ -1 +1 @@\n-before\n+after\n",
        "--- a/second.txt\n+++ b/second.txt\n@@ -1 +1 @@\n-wrong\n+after\n"
    );
    let mut client = McpClient::spawn(root.path());
    failure(&mut client, "apply_patch", json!({"patch_content": patch}));
    assert_eq!(
        fs::read(root.path().join("first.txt")).unwrap(),
        b"before\n"
    );
    assert_eq!(
        fs::read(root.path().join("second.txt")).unwrap(),
        b"actual\n"
    );
}

#[test]
fn patches_validate_every_path_and_do_not_modify_secrets_or_escape_root() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".env"), "secret\n").unwrap();
    fs::write(root.path().join("safe.txt"), "safe\n").unwrap();
    let mut client = McpClient::spawn(root.path());
    for path in [".env", "../outside.txt", "/tmp/outside.txt", ".git/config"] {
        let patch = format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-secret\n+changed\n");
        failure(&mut client, "apply_patch", json!({"patch_content": patch}));
    }
    failure(
        &mut client,
        "apply_patch",
        json!({"patch_content": "not a patch"}),
    );
    assert_eq!(fs::read(root.path().join(".env")).unwrap(), b"secret\n");
    assert_eq!(fs::read(root.path().join("safe.txt")).unwrap(), b"safe\n");
}

#[test]
fn patches_honor_target_directory_and_expected_hashes() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("service")).unwrap();
    fs::write(root.path().join("service/code.txt"), "old\n").unwrap();
    let mut client = McpClient::spawn(root.path());
    let info = success(
        &mut client,
        "file_info",
        json!({"path": "service/code.txt"}),
    );
    let patch = "--- a/code.txt\n+++ b/code.txt\n@@ -1 +1 @@\n-old\n+new\n";
    failure(
        &mut client,
        "apply_patch",
        json!({
            "patch_content": patch, "target_dir": "service", "expected_hashes": {"code.txt": "stale"}
        }),
    );
    assert_eq!(
        fs::read(root.path().join("service/code.txt")).unwrap(),
        b"old\n"
    );
    success(
        &mut client,
        "apply_patch",
        json!({
            "patch_content": patch, "target_dir": "service", "expected_hashes": {"code.txt": info["content_hash"]}
        }),
    );
    assert_eq!(
        fs::read(root.path().join("service/code.txt")).unwrap(),
        b"new\n"
    );
}
