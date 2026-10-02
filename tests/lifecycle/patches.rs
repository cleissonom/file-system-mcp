use super::*;

pub(super) fn check_patches(
    client: &mut McpClient,
    temp: &std::path::Path,
    repo_dir: &std::path::Path,
) {
    // 22. Tool Call: write_patch_file creates a new patch and auto-creates patches/
    let write_patch_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 22,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "auth-service-fix.patch",
                "content": "--- a/main.py\n+++ b/main.py\n@@ -1 +1,2 @@\n print('Auth service running')\n+print('Patch validated!')\n"
            }
        }
    }));
    assert_eq!(write_patch_resp["id"], 22);
    assert_eq!(write_patch_resp["result"]["isError"], Value::Null);
    assert!(
        write_patch_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Successfully saved patch to 'patches/auth-service-fix.patch'")
    );

    // 23. Tool Call: validate_patch on valid patch (dry-run only!)
    let val_patch_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_filename": "auth-service-fix.patch",
                "target_dir": "services/auth-service"
            }
        }
    }));
    assert_eq!(val_patch_resp["id"], 23);
    assert_eq!(val_patch_resp["result"]["isError"], Value::Null);
    let val_text = val_patch_resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(val_text.contains("applies cleanly"));
    assert!(val_text.contains("dry-run check passed"));
    // INVARIANT: working tree is untouched
    assert_eq!(
        fs::read_to_string(repo_dir.join("main.py")).unwrap(),
        "print('Auth service running')\n"
    );

    // 24. Tool Call: write_patch_file on existing patch without overwrite -> error
    let write_existing_patch = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 24,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "auth-service-fix.patch",
                "content": "--- a/main.py\n+++ b/main.py\n"
            }
        }
    }));
    assert_eq!(write_existing_patch["id"], 24);
    assert_eq!(write_existing_patch["result"]["isError"], true);
    assert!(
        write_existing_patch["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("already exists")
    );

    // 25. Tool Call: write_patch_file with overwrite: true -> success
    let overwrite_patch_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 25,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "auth-service-fix.patch",
                "content": "--- a/main.py\n+++ b/main.py\n@@ -1 +1,2 @@\n print('Auth service running')\n+print('Overwritten patch')\n",
                "overwrite": true
            }
        }
    }));
    assert_eq!(overwrite_patch_resp["id"], 25);
    assert_eq!(overwrite_patch_resp["result"]["isError"], Value::Null);

    // 26. Tool Call: write_patch_file rejecting non-patch extension
    let bad_patch_ext = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 26,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "exploit.py",
                "content": "print('malicious')"
            }
        }
    }));
    assert_eq!(bad_patch_ext["id"], 26);
    assert_eq!(bad_patch_ext["result"]["isError"], true);
    assert!(
        bad_patch_ext["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Invalid file extension")
    );

    // 27. Tool Call: write_patch_file rejecting directory traversal
    let patch_traversal = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 27,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "../escape.patch",
                "content": "diff escape"
            }
        }
    }));
    assert_eq!(patch_traversal["id"], 27);
    assert_eq!(patch_traversal["result"]["isError"], true);
    assert!(
        patch_traversal["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("traversal")
    );

    // 28. Tool Call: validate_patch on failing/conflicting patch
    client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 281,
        "method": "tools/call",
        "params": {
            "name": "write_patch_file",
            "arguments": {
                "filename": "conflict.patch",
                "content": "--- a/main.py\n+++ b/main.py\n@@ -1,2 +1,2 @@\n-nonexistent line\n+replaced\n"
            }
        }
    }));
    let conflict_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 28,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_filename": "conflict.patch",
                "target_dir": "services/auth-service"
            }
        }
    }));
    assert_eq!(conflict_resp["id"], 28);
    assert_eq!(conflict_resp["result"]["isError"], true);
    assert!(
        conflict_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("failed to apply")
    );
    // Invariant: file untouched
    assert_eq!(
        fs::read_to_string(repo_dir.join("main.py")).unwrap(),
        "print('Auth service running')\n"
    );

    // 29. Tool Call: validate_patch with directory traversal in target_dir
    let val_trav_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 29,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_filename": "auth-service-fix.patch",
                "target_dir": "../outside"
            }
        }
    }));
    assert_eq!(val_trav_resp["id"], 29);
    assert_eq!(val_trav_resp["result"]["isError"], true);
    assert!(
        val_trav_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("traversal")
    );

    // 30. Tool Call: validate_patch targeting denylisted directory
    let val_deny_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 30,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_filename": "auth-service-fix.patch",
                "target_dir": ".ssh"
            }
        }
    }));
    assert_eq!(val_deny_resp["id"], 30);
    assert_eq!(val_deny_resp["result"]["isError"], true);
    assert!(
        val_deny_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("denylist")
    );

    // 31. Tool Call: validate_patch using direct inline patch_content
    let val_inline_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 31,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_content": "--- a/main.py\n+++ b/main.py\n@@ -1 +1,2 @@\n print('Auth service running')\n+print('Inline test')\n",
                "target_dir": "services/auth-service"
            }
        }
    }));
    assert_eq!(val_inline_resp["id"], 31);
    assert_eq!(val_inline_resp["result"]["isError"], Value::Null);
    assert!(
        val_inline_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("applies cleanly")
    );
    assert_eq!(
        fs::read_to_string(repo_dir.join("main.py")).unwrap(),
        "print('Auth service running')\n"
    );

    // 32. Tool Call: validate_patch rejecting patch that targets denylisted file (.env)
    let val_deny_file_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 32,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_content": "--- a/.env\n+++ b/.env\n@@ -1 +1 @@\n-SECRET=1\n+SECRET=2\n",
                "target_dir": "services/auth-service"
            }
        }
    }));
    assert_eq!(val_deny_file_resp["id"], 32);
    assert_eq!(val_deny_file_resp["result"]["isError"], true);
    assert!(
        val_deny_file_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("denylist")
    );

    // 33. Tool Call: validate_patch on missing nested patch must NOT create directories
    let val_no_dir_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 33,
        "method": "tools/call",
        "params": {
            "name": "validate_patch",
            "arguments": {
                "patch_filename": "nonexistent_e2e_dir/missing.patch",
                "target_dir": "services/auth-service"
            }
        }
    }));
    assert_eq!(val_no_dir_resp["id"], 33);
    assert_eq!(val_no_dir_resp["result"]["isError"], true);
    assert!(!temp.join("patches").join("nonexistent_e2e_dir").exists());
}
