mod workspace_support;

use serde_json::json;
use std::fs;
use workspace_support::{McpClient, failure, success};

#[test]
fn workspace_tools_have_discoverable_schemas_and_accurate_annotations() {
    let root = tempfile::tempdir().unwrap();
    let mut client = McpClient::spawn(root.path());
    let response = client.send_request(json!({"id": 1, "method": "tools/list"}));
    let tools = response["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 15);
    for name in [
        "workspace_info",
        "file_info",
        "write_file",
        "edit_file",
        "apply_patch",
        "create_directory",
        "copy_path",
        "move_path",
        "delete_path",
    ] {
        let tool = tools.iter().find(|tool| tool["name"] == name).expect(name);
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
    for tool in tools {
        let read_only = matches!(
            tool["name"].as_str().unwrap(),
            "workspace_info"
                | "file_info"
                | "read_file"
                | "list_directory"
                | "search_files"
                | "validate_patch"
        );
        assert_eq!(tool["annotations"]["readOnlyHint"], read_only);
    }
}

#[test]
fn workspace_info_reports_the_explicit_root_and_write_permission() {
    let root = tempfile::tempdir().unwrap();
    let mut client = McpClient::spawn(root.path());
    let info = success(&mut client, "workspace_info", json!({}));
    assert_eq!(
        info["root"],
        root.path().canonicalize().unwrap().to_str().unwrap()
    );
    assert_eq!(info["writable"], true);
    assert!(info["limits"]["max_text_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn validation_schema_admits_inline_content_without_a_stored_patch() {
    let root = tempfile::tempdir().unwrap();
    let mut client = McpClient::spawn(root.path());
    let response = client.send_request(json!({"id": 1, "method": "tools/list"}));
    let validator = response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["name"] == "validate_patch")
        .unwrap();
    let schema = &validator["inputSchema"];
    assert!(
        !schema["required"]
            .as_array()
            .is_some_and(|required| required.contains(&json!("patch_filename")))
    );
    assert!(
        schema["anyOf"]
            .as_array()
            .unwrap()
            .iter()
            .any(|alternative| alternative["required"] == json!(["patch_content"]))
    );
    fs::write(root.path().join("code.txt"), "before\n").unwrap();
    let result = workspace_support::call(
        &mut client,
        "validate_patch",
        json!({
            "patch_content": "--- a/code.txt\n+++ b/code.txt\n@@ -1 +1 @@\n-before\n+after\n"
        }),
    );
    assert_ne!(result["isError"], true);
    assert_eq!(
        fs::read_to_string(root.path().join("code.txt")).unwrap(),
        "before\n"
    );
}

#[test]
fn files_can_be_created_inspected_edited_and_replaced_without_lost_updates() {
    let root = tempfile::tempdir().unwrap();
    let mut client = McpClient::spawn(root.path());
    success(&mut client, "create_directory", json!({"path": "src"}));
    let created = success(
        &mut client,
        "write_file",
        json!({"path": "src/config.txt", "content": "alpha\n"}),
    );
    let info = success(&mut client, "file_info", json!({"path": "src/config.txt"}));
    assert_eq!(info["type"], "file");
    assert_eq!(info["size"], 6);
    assert_eq!(info["content_hash"].as_str().unwrap().len(), 64);
    assert_eq!(info["content_hash"], created["content_hash"]);
    failure(
        &mut client,
        "write_file",
        json!({"path": "src/config.txt", "content": "oops"}),
    );
    let edited = success(
        &mut client,
        "edit_file",
        json!({
            "path": "src/config.txt", "expected_hash": info["content_hash"],
            "edits": [{"old_text": "alpha", "new_text": "beta"}]
        }),
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/config.txt")).unwrap(),
        "beta\n"
    );
    failure(
        &mut client,
        "write_file",
        json!({
            "path": "src/config.txt", "content": "stale", "overwrite": true,
            "expected_hash": info["content_hash"]
        }),
    );
    success(
        &mut client,
        "write_file",
        json!({
            "path": "src/config.txt", "content": "gamma\n", "overwrite": true,
            "expected_hash": edited["content_hash"]
        }),
    );
    assert_eq!(
        fs::read_to_string(root.path().join("src/config.txt")).unwrap(),
        "gamma\n"
    );
}

#[test]
fn editing_rejects_missing_or_ambiguous_matches_without_partial_changes() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("code.txt"), "same same\n").unwrap();
    let mut client = McpClient::spawn(root.path());
    let message = failure(
        &mut client,
        "edit_file",
        json!({
            "path": "code.txt", "edits": [{"old_text": "same", "new_text": "new"}]
        }),
    );
    assert!(message.contains("match"));
    failure(
        &mut client,
        "edit_file",
        json!({
            "path": "code.txt", "edits": [
                {"old_text": "same same", "new_text": "changed"},
                {"old_text": "missing", "new_text": "oops"}
            ]
        }),
    );
    assert_eq!(
        fs::read_to_string(root.path().join("code.txt")).unwrap(),
        "same same\n"
    );
    success(
        &mut client,
        "edit_file",
        json!({
            "path": "code.txt", "edits": [{"old_text": "same", "new_text": "new", "replace_all": true}]
        }),
    );
    assert_eq!(
        fs::read_to_string(root.path().join("code.txt")).unwrap(),
        "new new\n"
    );
}

#[test]
fn directories_can_be_copied_moved_and_explicitly_deleted_recursively() {
    let root = tempfile::tempdir().unwrap();
    let mut client = McpClient::spawn(root.path());
    success(
        &mut client,
        "create_directory",
        json!({"path": "tree/sub", "recursive": true}),
    );
    success(
        &mut client,
        "write_file",
        json!({"path": "tree/sub/code.txt", "content": "body"}),
    );
    success(
        &mut client,
        "copy_path",
        json!({"source": "tree", "destination": "copy", "recursive": true}),
    );
    assert_eq!(
        fs::read(root.path().join("copy/sub/code.txt")).unwrap(),
        b"body"
    );
    success(
        &mut client,
        "move_path",
        json!({"source": "copy", "destination": "moved"}),
    );
    assert!(!root.path().join("copy").exists());
    failure(&mut client, "delete_path", json!({"path": "moved"}));
    assert!(root.path().join("moved/sub/code.txt").exists());
    success(
        &mut client,
        "delete_path",
        json!({"path": "moved", "recursive": true}),
    );
    assert!(!root.path().join("moved").exists());
    assert!(root.path().join("tree/sub/code.txt").exists());
}

#[test]
fn copying_and_moving_require_explicit_overwrite() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("source.txt"), "source").unwrap();
    fs::write(root.path().join("destination.txt"), "destination").unwrap();
    let mut client = McpClient::spawn(root.path());
    for tool in ["copy_path", "move_path"] {
        failure(
            &mut client,
            tool,
            json!({"source": "source.txt", "destination": "destination.txt"}),
        );
        assert_eq!(
            fs::read(root.path().join("destination.txt")).unwrap(),
            b"destination"
        );
    }
    success(
        &mut client,
        "copy_path",
        json!({"source": "source.txt", "destination": "destination.txt", "overwrite": true}),
    );
    assert_eq!(
        fs::read(root.path().join("destination.txt")).unwrap(),
        b"source"
    );
    success(
        &mut client,
        "move_path",
        json!({"source": "source.txt", "destination": "renamed.txt"}),
    );
    assert!(!root.path().join("source.txt").exists());
}

#[test]
fn directory_copy_preserves_regular_permission_bits() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("source/sub")).unwrap();
    fs::write(root.path().join("source/sub/code.txt"), "content").unwrap();
    for (path, mode) in [
        ("source", 0o750),
        ("source/sub", 0o510),
        ("source/sub/code.txt", 0o640),
    ] {
        fs::set_permissions(root.path().join(path), fs::Permissions::from_mode(mode)).unwrap();
    }
    let mut client = McpClient::spawn(root.path());
    success(
        &mut client,
        "copy_path",
        json!({"source": "source", "destination": "copy", "recursive": true}),
    );
    for (path, mode) in [
        ("copy", 0o750),
        ("copy/sub", 0o510),
        ("copy/sub/code.txt", 0o640),
    ] {
        assert_eq!(
            fs::metadata(root.path().join(path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            mode
        );
    }
}

#[test]
fn mutations_validate_parameter_types_and_reject_special_files() {
    let root = tempfile::tempdir().unwrap();
    let mut client = McpClient::spawn(root.path());
    failure(
        &mut client,
        "write_file",
        json!({"path": "file.txt", "content": "text", "overwrite": "yes"}),
    );
    failure(
        &mut client,
        "create_directory",
        json!({"path": "dir", "recursive": 1}),
    );
    failure(
        &mut client,
        "edit_file",
        json!({"path": "file.txt", "edits": []}),
    );
    assert!(!root.path().join("file.txt").exists());
    assert!(!root.path().join("dir").exists());
    #[cfg(unix)]
    {
        let socket_path = root.path().join("socket");
        let _socket = std::os::unix::net::UnixListener::bind(&socket_path).unwrap();
        failure(&mut client, "file_info", json!({"path": "socket"}));
        failure(
            &mut client,
            "write_file",
            json!({"path": "socket", "content": "text", "overwrite": true}),
        );
    }
}
