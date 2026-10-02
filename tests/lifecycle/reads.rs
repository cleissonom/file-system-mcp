use super::*;

pub(super) fn check_reads(client: &mut McpClient) {
    // 5. Tool Call: read_file on valid file
    let read_ok = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 4,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": "README.md"
            }
        }
    }));
    assert_eq!(read_ok["id"], 4);
    assert_eq!(read_ok["result"]["isError"], Value::Null);
    let read_text = read_ok["result"]["content"][0]["text"].as_str().unwrap();
    assert!(read_text.contains("# Welcome to the Workspace"));

    // 6. Tool Call: read_file on denylisted .env
    let read_env = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 5,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": ".env"
            }
        }
    }));
    assert_eq!(read_env["result"]["isError"], true);
    assert!(
        read_env["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("denylist")
    );

    // 7. Tool Call: read_file on denylisted bastion.sh
    let read_bastion = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 6,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": ".helpers/bastion.sh"
            }
        }
    }));
    assert_eq!(read_bastion["result"]["isError"], true);
    assert!(
        read_bastion["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("denylist")
    );

    // 8. Tool Call: read_file on gitignored file in sub-repo
    let read_gitignored = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 7,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": "services/auth-service/debug.log"
            }
        }
    }));
    assert_eq!(read_gitignored["result"]["isError"], true);
    assert!(
        read_gitignored["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(".gitignore")
    );

    // 9. Tool Call: read_file with path traversal attempt
    let read_traversal = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 8,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": "../../../etc/passwd"
            }
        }
    }));
    assert_eq!(read_traversal["result"]["isError"], true);
    assert!(
        read_traversal["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("traversal")
    );

    // 10. Tool Call: list_directory
    let list_dir_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 9,
        "method": "tools/call",
        "params": {
            "name": "list_directory",
            "arguments": {
                "path": ".",
                "depth": 3
            }
        }
    }));
    assert_eq!(list_dir_resp["id"], 9);
    let list_text = list_dir_resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    // Should include README.md and auth-service/main.py
    assert!(list_text.contains("README.md"));
    assert!(list_text.contains("main.py"));
    // Must NOT include .env, bastion.sh, .git directory, or debug.log
    assert!(!list_text.contains(".env"));
    assert!(!list_text.contains("bastion.sh"));
    assert!(
        !list_text.contains("/.git/") && !list_text.contains("[DIR]  services/auth-service/.git/")
    );
    assert!(!list_text.contains("debug.log"));
    assert!(!list_text.contains("bundle.js"));

    // 11. Tool Call: search_files
    let search_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 10,
        "method": "tools/call",
        "params": {
            "name": "search_files",
            "arguments": {
                "query": "Auth service"
            }
        }
    }));
    assert_eq!(search_resp["id"], 10);
    let search_text = search_resp["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(search_text.contains("main.py:1: print('Auth service running')"));

    // Search for string inside .env - must return no matches!
    let search_secret = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 11,
        "method": "tools/call",
        "params": {
            "name": "search_files",
            "arguments": {
                "query": "SECRET_KEY"
            }
        }
    }));
    let search_secret_text = search_secret["result"]["content"][0]["text"]
        .as_str()
        .unwrap();
    assert!(search_secret_text.contains("No matching lines found"));

    // 12. Tool Call: read_file with leading slash (e.g. /README.md)
    let read_slash = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 12,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": "/README.md"
            }
        }
    }));
    assert_eq!(read_slash["id"], 12);
    assert_eq!(read_slash["result"]["isError"], Value::Null);
    assert!(
        read_slash["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("# Welcome to the Workspace")
    );

    // 13. Tool Call: search_files on gitignored directory directly (must be rejected)
    let search_gitignored = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 13,
        "method": "tools/call",
        "params": {
            "name": "search_files",
            "arguments": {
                "query": "compiled",
                "path": "services/auth-service/build"
            }
        }
    }));
    assert_eq!(search_gitignored["result"]["isError"], true);
    assert!(
        search_gitignored["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains(".gitignore")
    );

    // 14. Tool Call: search_files with path-prefixed file_pattern
    let search_glob = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 14,
        "method": "tools/call",
        "params": {
            "name": "search_files",
            "arguments": {
                "query": "Auth service",
                "file_pattern": "services/**/*.py"
            }
        }
    }));
    assert_eq!(search_glob["id"], 14);
    assert!(
        search_glob["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("main.py:1: print('Auth service running')")
    );
}
