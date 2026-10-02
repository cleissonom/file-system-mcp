mod support;

use serde_json::{Value, json};
use std::fs;
use support::McpClient;

#[test]
fn test_mcp_cli_version_matches_package() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_file-system-mcp"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        format!("file-system-mcp {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn test_mcp_cli_custom_plans_and_patches_dirs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp = temp_dir.path();

    fs::write(temp.join("code.txt"), "hello").unwrap();

    let root_str = temp.to_str().unwrap();
    let mut client = McpClient::spawn_with_options(
        &[
            "--root",
            root_str,
            "--plans-dir",
            "my_plans",
            "--patches-dir",
            "my_patches",
        ],
        &[],
        None,
    );

    let init_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    }));
    assert_eq!(init_resp["id"], 1);

    let write_plan = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "custom.plan.md",
                "content": "# Custom Plan"
            }
        }
    }));
    assert_eq!(write_plan["id"], 2);
    assert_eq!(write_plan["result"]["isError"], Value::Null);
    assert!(temp.join("my_plans/custom.plan.md").is_file());

    let write_patch = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "custom.patch",
                "content": "--- a/code.txt\n+++ b/code.txt\n@@ -1 +1 @@\n-hello\n+world\n"
            }
        }
    }));
    assert_eq!(write_patch["id"], 3);
    assert_eq!(write_patch["result"]["isError"], Value::Null);
    assert!(temp.join("my_patches/custom.patch").is_file());
}

#[test]
fn test_mcp_env_var_paths() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp = temp_dir.path();

    let root_str = temp.to_str().unwrap();
    let mut client = McpClient::spawn_with_options(
        &[],
        &[
            ("WORKSPACE_ROOT", root_str),
            ("PLANS_DIR", "env_plans"),
            ("PATCHES_DIR", "env_patches"),
        ],
        None,
    );

    let init_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    }));
    assert_eq!(init_resp["id"], 1);

    let write_plan = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "env.plan.md",
                "content": "# Env Plan"
            }
        }
    }));
    assert_eq!(write_plan["id"], 2);
    assert_eq!(write_plan["result"]["isError"], Value::Null);
    assert!(temp.join("env_plans/env.plan.md").is_file());
}

#[test]
fn test_mcp_relative_root() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp = temp_dir.path();
    fs::write(temp.join("relative_file.txt"), "relative content").unwrap();

    let mut client = McpClient::spawn_with_options(&["--root", "."], &[], Some(temp));

    let init_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    }));
    assert_eq!(init_resp["id"], 1);

    let read_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {"path": "relative_file.txt"}
        }
    }));
    assert_eq!(read_resp["id"], 2);
    assert_eq!(read_resp["result"]["isError"], Value::Null);
    assert!(
        read_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("relative content")
    );
}

#[test]
fn test_mcp_env_var_empty_fallback() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp = temp_dir.path();
    fs::write(temp.join("from_mcp_root.txt"), "hello mcp root").unwrap();

    let root_str = temp.to_str().unwrap();
    let mut client = McpClient::spawn_with_options(
        &[],
        &[("WORKSPACE_ROOT", ""), ("MCP_WORKSPACE_ROOT", root_str)],
        None,
    );

    let init_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    }));
    assert_eq!(init_resp["id"], 1);

    let read_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {"path": "from_mcp_root.txt"}
        }
    }));
    assert_eq!(read_resp["id"], 2);
    assert_eq!(read_resp["result"]["isError"], Value::Null);
    assert!(
        read_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("hello mcp root")
    );
}

#[test]
fn test_mcp_cli_nested_plans_and_patches_dirs() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp = temp_dir.path();
    fs::write(temp.join("code.txt"), "val_initial\n").unwrap();

    let root_str = temp.to_str().unwrap();
    let mut client = McpClient::spawn_with_options(
        &[
            "--root",
            root_str,
            "--plans-dir",
            "docs/plans",
            "--patches-dir",
            "sub/patches",
        ],
        &[],
        None,
    );

    let init_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {"name": "test-client", "version": "1.0"}
        }
    }));
    assert_eq!(init_resp["id"], 1);

    // Write plan with relative nested path
    let write_plan1 = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "docs/plans/arch.plan.md",
                "content": "# Arch Plan 1"
            }
        }
    }));
    assert_eq!(write_plan1["id"], 2);
    assert_eq!(write_plan1["result"]["isError"], Value::Null);
    assert!(temp.join("docs/plans/arch.plan.md").is_file());
    // Ensure no doubled directory
    assert!(!temp.join("docs/plans/docs").exists());

    // Write plan with root-relative nested path
    let write_plan2 = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "/docs/plans/arch2.plan.md",
                "content": "# Arch Plan 2"
            }
        }
    }));
    assert_eq!(write_plan2["id"], 3);
    assert_eq!(write_plan2["result"]["isError"], Value::Null);
    assert!(temp.join("docs/plans/arch2.plan.md").is_file());

    // Write patch with relative nested path
    let write_patch1 = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "sub/patches/fix.patch",
                "content": "--- a/code.txt\n+++ b/code.txt\n@@ -1 +1 @@\n-val_initial\n+val_patched\n"
            }
        }
    }));
    assert_eq!(write_patch1["id"], 4);
    assert_eq!(write_patch1["result"]["isError"], Value::Null);
    assert!(temp.join("sub/patches/fix.patch").is_file());
    assert!(!temp.join("sub/patches/sub").exists());

    // Validate patch with root-relative path
    let val_patch = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_filename": "/sub/patches/fix.patch",
                "target_dir": "."
            }
        }
    }));
    assert_eq!(val_patch["id"], 5);
    assert_eq!(val_patch["result"]["isError"], Value::Null);
}
