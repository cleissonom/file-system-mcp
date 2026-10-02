use super::*;

#[test]
fn test_tool_write_plan_file_creation_and_auto_dir() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    // Ensure plans directory does not exist initially
    assert!(!canonical_root.join("plans").exists());

    let res = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "my-plan.md",
            "content": "# My Architecture Plan\nDetails here."
        })),
    );
    assert!(res.is_error.is_none());
    assert!(canonical_root.join("plans/my-plan.md").is_file());
    let read_back = fs::read_to_string(canonical_root.join("plans/my-plan.md")).unwrap();
    assert_eq!(read_back, "# My Architecture Plan\nDetails here.");

    // Check subpath creation
    let res_sub = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "nested/service.markdown",
            "content": "# Nested Plan"
        })),
    );
    assert!(res_sub.is_error.is_none());
    assert!(
        canonical_root
            .join("plans/nested/service.markdown")
            .is_file()
    );
}

#[test]
fn test_tool_write_plan_file_overwrite_guard() {
    let (_td, temp) = temp_workspace();
    fs::create_dir_all(temp.join("plans")).unwrap();
    fs::write(temp.join("plans/existing.md"), "original content").unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. Overwrite not specified (defaults to false) -> error
    let res = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "existing.md",
            "content": "new content"
        })),
    );
    assert_eq!(res.is_error, Some(true));
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("already exists"));
            assert!(text.contains("Set overwrite to true"));
        }
    }
    assert_eq!(
        fs::read_to_string(canonical_root.join("plans/existing.md")).unwrap(),
        "original content"
    );

    // 2. Overwrite explicitly false -> error
    let res_false = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "existing.md",
            "content": "new content",
            "overwrite": false
        })),
    );
    assert_eq!(res_false.is_error, Some(true));

    // 3. Overwrite explicitly true -> success
    let res_true = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "existing.md",
            "content": "updated content",
            "overwrite": true
        })),
    );
    assert!(res_true.is_error.is_none());
    assert_eq!(
        fs::read_to_string(canonical_root.join("plans/existing.md")).unwrap(),
        "updated content"
    );
}

#[test]
fn test_tool_write_plan_file_extension_rejection() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    for bad_ext in &[
        "plan.py",
        "script.sh",
        "code.rs",
        "data.json",
        "config.yaml",
        "no_ext",
    ] {
        let res = execute_tool(
            &canonical_root,
            "write_plan_file",
            Some(&json!({
                "filename": bad_ext,
                "content": "content"
            })),
        );
        assert_eq!(res.is_error, Some(true));
        match &res.content[0] {
            crate::protocol::ToolContent::Text { text } => {
                assert!(text.contains("Invalid file extension") || text.contains("Invalid path"));
            }
        }
    }
}

#[test]
fn test_tool_write_plan_file_traversal_rejection() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    let traversal_paths = &[
        "../escape.md",
        "../../escape.md",
        "plans/../../escape.md",
        "sub/../../escape.md",
    ];

    for path in traversal_paths {
        let res = execute_tool(
            &canonical_root,
            "write_plan_file",
            Some(&json!({
                "filename": path,
                "content": "evil content"
            })),
        );
        assert_eq!(res.is_error, Some(true));
        match &res.content[0] {
            crate::protocol::ToolContent::Text { text } => {
                assert!(text.contains("traversal"));
            }
        }
    }
}

#[test]
fn test_tool_write_plan_file_size_cap_and_params() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. Missing filename
    let res_no_file = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({"content": "hello"})),
    );
    assert_eq!(res_no_file.is_error, Some(true));

    // 2. Missing content
    let res_no_content = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({"filename": "plan.md"})),
    );
    assert_eq!(res_no_content.is_error, Some(true));

    // 3. Exceeds size cap (500 KB)
    let large_content = "a".repeat(500 * 1024 + 1);
    let res_large = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "large.md",
            "content": large_content
        })),
    );
    assert_eq!(res_large.is_error, Some(true));
    match &res_large.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("exceeds safety limit"));
        }
    }
}

#[test]
fn test_tool_write_plan_file_path_confinement_and_symlinks() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. External absolute path rejected
    let res_ext = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "/etc/passwd.md",
            "content": "# external"
        })),
    );
    assert_eq!(res_ext.is_error, Some(true));

    // 2. Full absolute path inside plans/ accepted
    let full_abs = canonical_root.join("plans/abs_plan.md");
    let res_abs = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": full_abs.to_string_lossy(),
            "content": "# full abs plan"
        })),
    );
    assert!(res_abs.is_error.is_none());
    assert!(canonical_root.join("plans/abs_plan.md").is_file());

    // 3. Leading ./plans/ prefix accepted and normalized
    let res_dot = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "./plans/dot_plan.md",
            "content": "# dot plan"
        })),
    );
    assert!(res_dot.is_error.is_none());
    assert!(canonical_root.join("plans/dot_plan.md").is_file());

    // 4. Trailing slash rejected
    let res_slash = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": "dir_plan.md/",
            "content": "# dir plan"
        })),
    );
    assert_eq!(res_slash.is_error, Some(true));

    // 5. Backslash traversal rejected
    let res_bs = execute_tool(
        &canonical_root,
        "write_plan_file",
        Some(&json!({
            "filename": r"..\bs_escape.md",
            "content": "# backslash"
        })),
    );
    assert_eq!(res_bs.is_error, Some(true));

    // 6. Symlink rejection
    #[cfg(unix)]
    {
        let dangling = canonical_root.join("plans/sym_escape.md");
        let outside = temp.join("outside.md");
        std::os::unix::fs::symlink(&outside, &dangling).unwrap();
        let res_sym = execute_tool(
            &canonical_root,
            "write_plan_file",
            Some(&json!({
                "filename": "sym_escape.md",
                "content": "# symlink attack",
                "overwrite": true
            })),
        );
        assert_eq!(res_sym.is_error, Some(true));
    }
}
