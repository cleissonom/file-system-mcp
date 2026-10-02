use super::*;

pub(super) fn check_plans(client: &mut McpClient) {
    // 15. Tool Call: write_plan_file creates a new plan and auto-creates plans/
    let write_plan_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 15,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "parser-scalability.plan.md",
                "content": "# Architecture Plan\nScalable parser implementation details."
            }
        }
    }));
    assert_eq!(write_plan_resp["id"], 15);
    assert_eq!(write_plan_resp["result"]["isError"], Value::Null);
    assert!(
        write_plan_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Successfully saved plan to 'plans/parser-scalability.plan.md'")
    );

    // 16. Tool Call: read_file on newly written plan
    let read_plan_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 16,
        "method": "tools/call",
        "params": {
            "name": "read_file",
            "arguments": {
                "path": "plans/parser-scalability.plan.md"
            }
        }
    }));
    assert_eq!(read_plan_resp["id"], 16);
    assert_eq!(read_plan_resp["result"]["isError"], Value::Null);
    assert!(
        read_plan_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("# Architecture Plan")
    );

    // 17. Tool Call: write_plan_file on existing file without overwrite -> error
    let write_existing_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 17,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "parser-scalability.plan.md",
                "content": "# Overwrite attempt without flag"
            }
        }
    }));
    assert_eq!(write_existing_resp["id"], 17);
    assert_eq!(write_existing_resp["result"]["isError"], true);
    assert!(
        write_existing_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("already exists")
    );

    // 18. Tool Call: write_plan_file with overwrite=true -> success
    let overwrite_plan_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 18,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "parser-scalability.plan.md",
                "content": "# Updated Plan Content",
                "overwrite": true
            }
        }
    }));
    assert_eq!(overwrite_plan_resp["id"], 18);
    assert_eq!(overwrite_plan_resp["result"]["isError"], Value::Null);

    // 19. Tool Call: write_plan_file rejecting non-markdown extension
    let bad_ext_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 19,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "script.py",
                "content": "print('malicious')"
            }
        }
    }));
    assert_eq!(bad_ext_resp["id"], 19);
    assert_eq!(bad_ext_resp["result"]["isError"], true);
    assert!(
        bad_ext_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("Invalid file extension")
    );

    // 20. Tool Call: write_plan_file rejecting directory traversal
    let traversal_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 20,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "../escape.md",
                "content": "# Escape attempt"
            }
        }
    }));
    assert_eq!(traversal_resp["id"], 20);
    assert_eq!(traversal_resp["result"]["isError"], true);
    assert!(
        traversal_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("traversal")
    );

    // 21. Tool Call: write_plan_file rejecting external absolute path
    let ext_path_resp = client.send_request(json!({
        "jsonrpc": "2.0",
        "id": 21,
        "method": "tools/call",
        "params": {
            "name": "write_plan_file",
            "arguments": {
                "filename": "/etc/passwd.md",
                "content": "# External write attempt"
            }
        }
    }));
    assert_eq!(ext_path_resp["id"], 21);
    assert_eq!(ext_path_resp["result"]["isError"], true);
    assert!(
        ext_path_resp["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("outside the workspace root")
    );
}
