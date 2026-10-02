mod lifecycle;
mod support;

use serde_json::json;
use std::fs;
use support::McpClient;

#[test]
fn test_mcp_e2e_full_lifecycle_and_security() {
    let temp_dir = tempfile::tempdir().unwrap();
    let temp = temp_dir.path();

    // Setup files
    fs::write(
        temp.join("README.md"),
        "# Welcome to the Workspace\nThis is a safe file.\n",
    )
    .unwrap();

    // Denylisted files
    fs::write(temp.join(".env"), "SECRET_KEY=12345").unwrap();
    fs::write(temp.join(".env.production"), "DATABASE_URL=postgres://...").unwrap();
    fs::write(temp.join("server.pem"), "-----BEGIN CERTIFICATE-----").unwrap();
    fs::write(temp.join("deploy.key"), "-----BEGIN PRIVATE KEY-----").unwrap();
    fs::create_dir_all(temp.join(".helpers")).unwrap();
    fs::write(
        temp.join(".helpers/bastion.sh"),
        "#!/bin/bash\necho bastion",
    )
    .unwrap();
    fs::create_dir_all(temp.join(".ssh")).unwrap();
    fs::write(temp.join(".ssh/id_rsa"), "secret ssh key").unwrap();

    // Sub-repo with .git and .gitignore
    let repo_dir = temp.join("services/auth-service");
    fs::create_dir_all(repo_dir.join(".git")).unwrap();
    fs::create_dir_all(repo_dir.join("build")).unwrap();
    fs::write(repo_dir.join(".gitignore"), "*.log\nbuild/\n.cache/\n").unwrap();
    fs::write(repo_dir.join("main.py"), "print('Auth service running')\n").unwrap();
    fs::write(
        repo_dir.join("debug.log"),
        "2026-09-22 Sensitive debug token",
    )
    .unwrap();
    fs::write(repo_dir.join("build/bundle.js"), "compiled code").unwrap();

    let mut client = McpClient::spawn(temp);

    // 1. Initialize
    let init_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": {
                "name": "openai-tunnel-client",
                "version": "1.0.0"
            }
        }
    }));
    assert_eq!(init_resp["id"], 1);
    assert_eq!(init_resp["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(init_resp["result"]["serverInfo"]["name"], "file-system-mcp");

    // 2. Initialized notification
    client.send_notification(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }));

    // 3. Ping
    let ping_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "ping"
    }));
    assert_eq!(ping_resp["id"], 2);
    assert!(ping_resp["result"].is_object());

    // 4. Tools list
    let list_tools_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 3,
        "method": "tools/list",
        "params": {}
    }));
    assert_eq!(list_tools_resp["id"], 3);
    let tools = list_tools_resp["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 15);
    let tool_names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(tool_names.contains(&"read_file"));
    assert!(tool_names.contains(&"list_directory"));
    assert!(tool_names.contains(&"search_files"));
    assert!(tool_names.contains(&"write_plan_file"));
    assert!(tool_names.contains(&"write_patch_file"));
    assert!(tool_names.contains(&"validate_patch"));

    lifecycle::check_tools(&mut client, temp, &repo_dir);
}
