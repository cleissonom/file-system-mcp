use super::*;

#[test]
fn test_tools_list() {
    let tools = get_available_tools();
    assert_eq!(tools.len(), 15);
    assert_eq!(tools[0].name, "read_file");
    assert_eq!(tools[1].name, "list_directory");
    assert_eq!(tools[2].name, "search_files");
    assert_eq!(tools[3].name, "write_plan_file");
    assert_eq!(tools[4].name, "write_patch_file");
    assert_eq!(tools[5].name, "validate_patch");
}

#[test]
fn test_tool_read_file() {
    let (_td, temp) = temp_workspace();
    fs::write(temp.join("hello.txt"), "Hello, world! Welcome to MCP.").unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    // Normal read
    let res = execute_tool(
        &canonical_root,
        "read_file",
        Some(&json!({"path": "hello.txt"})),
    );
    assert!(res.is_error.is_none());
    assert_eq!(
        res.content[0],
        crate::protocol::ToolContent::Text {
            text: "Hello, world! Welcome to MCP.".to_string()
        }
    );

    // Offset and limit read
    let res = execute_tool(
        &canonical_root,
        "read_file",
        Some(&json!({
            "path": "hello.txt",
            "offset": 7,
            "limit": 5
        })),
    );
    assert!(res.is_error.is_none());
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.starts_with("world"));
            assert!(text.contains("[File truncated: showing bytes 7-12 of 29"));
        }
    }

    // Leading slash read
    let res = execute_tool(
        &canonical_root,
        "read_file",
        Some(&json!({"path": "/hello.txt"})),
    );
    assert!(res.is_error.is_none());

    // Empty file with offset > 0
    fs::write(temp.join("empty.txt"), "").unwrap();
    let res = execute_tool(
        &canonical_root,
        "read_file",
        Some(&json!({"path": "empty.txt", "offset": 10})),
    );
    assert!(res.is_error.is_none());
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("exceeds file length"));
        }
    }

    // Traversal error
    let res = execute_tool(
        &canonical_root,
        "read_file",
        Some(&json!({"path": "../etc/passwd"})),
    );
    assert_eq!(res.is_error, Some(true));
}

#[test]
fn test_tool_list_directory() {
    let (_td, temp) = temp_workspace();
    fs::create_dir_all(temp.join("folder1")).unwrap();
    fs::write(temp.join("folder1/file1.txt"), "123").unwrap();
    fs::write(temp.join("root.txt"), "abc").unwrap();
    fs::write(temp.join(".env"), "SECRET=true").unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    let res = execute_tool(
        &canonical_root,
        "list_directory",
        Some(&json!({"path": "."})),
    );
    assert!(res.is_error.is_none());
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("[DIR]  folder1/"));
            assert!(text.contains("[FILE] root.txt (3 bytes)"));
            assert!(!text.contains(".env"));
        }
    }
}

#[test]
fn test_tool_search_files() {
    let (_td, temp) = temp_workspace();
    fs::create_dir_all(temp.join("src")).unwrap();
    fs::write(
        temp.join("src/lib.rs"),
        "fn calculate_sum() -> u32 {\n    42\n}\n",
    )
    .unwrap();
    fs::write(
        temp.join("src/main.rs"),
        "fn main() {\n    let sum = calculate_sum();\n}\n",
    )
    .unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    let res = execute_tool(
        &canonical_root,
        "search_files",
        Some(&json!({
            "query": "calculate_sum"
        })),
    );
    assert!(res.is_error.is_none());
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("src/lib.rs:1: fn calculate_sum"));
            assert!(text.contains("src/main.rs:2:     let sum = calculate_sum();"));
        }
    }

    // Test with file pattern
    let res = execute_tool(
        &canonical_root,
        "search_files",
        Some(&json!({
            "query": "calculate_sum",
            "file_pattern": "*main.rs"
        })),
    );
    assert!(res.is_error.is_none());
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(!text.contains("src/lib.rs"));
            assert!(text.contains("src/main.rs:2:     let sum = calculate_sum();"));
        }
    }

    // Test with file pattern containing directory prefix
    let res = execute_tool(
        &canonical_root,
        "search_files",
        Some(&json!({
            "query": "calculate_sum",
            "file_pattern": "src/*.rs"
        })),
    );
    assert!(res.is_error.is_none());
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("src/lib.rs:1: fn calculate_sum"));
            assert!(text.contains("src/main.rs:2:     let sum = calculate_sum();"));
        }
    }

    // Test search_files on gitignored directory
    fs::write(temp.join(".gitignore"), "ignored_dir/\n").unwrap();
    fs::create_dir_all(temp.join("ignored_dir")).unwrap();
    fs::write(
        temp.join("ignored_dir/secret.txt"),
        "calculate_sum in secret",
    )
    .unwrap();
    let res = execute_tool(
        &canonical_root,
        "search_files",
        Some(&json!({
            "query": "calculate_sum",
            "path": "ignored_dir"
        })),
    );
    assert_eq!(res.is_error, Some(true));
}
