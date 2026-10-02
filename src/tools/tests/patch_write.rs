use super::*;

#[test]
fn test_tool_write_patch_file_creation_and_auto_dir() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    assert!(!canonical_root.join("patches").exists());

    let res = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "fix.patch",
            "content": "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+new\n"
        })),
    );
    assert!(res.is_error.is_none());
    assert!(canonical_root.join("patches").is_dir());
    assert!(canonical_root.join("patches/fix.patch").is_file());
}

#[test]
fn test_tool_write_patch_file_overwrite_guard() {
    let (_td, temp) = temp_workspace();
    fs::create_dir_all(temp.join("patches")).unwrap();
    fs::write(temp.join("patches/existing.patch"), "original patch").unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. Default overwrite (false) -> error
    let res = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "existing.patch",
            "content": "new patch"
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
        fs::read_to_string(canonical_root.join("patches/existing.patch")).unwrap(),
        "original patch"
    );

    // 2. Overwrite explicitly false -> error
    let res_false = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "existing.patch",
            "content": "new patch",
            "overwrite": false
        })),
    );
    assert_eq!(res_false.is_error, Some(true));

    // 3. Overwrite true -> success
    let res_true = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "existing.patch",
            "content": "updated patch",
            "overwrite": true
        })),
    );
    assert!(res_true.is_error.is_none());
    assert_eq!(
        fs::read_to_string(canonical_root.join("patches/existing.patch")).unwrap(),
        "updated patch"
    );
}

#[test]
fn test_tool_write_patch_file_extension_rejection() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    for bad_ext in &[
        "fix.py",
        "script.sh",
        "code.rs",
        "data.json",
        "plan.md",
        "no_ext",
    ] {
        let res = execute_tool(
            &canonical_root,
            "write_patch_file",
            Some(&json!({
                "filename": bad_ext,
                "content": "diff content"
            })),
        );
        assert_eq!(res.is_error, Some(true));
        match &res.content[0] {
            crate::protocol::ToolContent::Text { text } => {
                assert!(text.contains("Invalid file extension") || text.contains("Invalid path"));
            }
        }
    }

    // Valid .diff extension
    let res_diff = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "valid.diff",
            "content": "diff content"
        })),
    );
    assert!(res_diff.is_error.is_none());
    assert!(canonical_root.join("patches/valid.diff").is_file());
}

#[test]
fn test_tool_write_patch_file_traversal_rejection() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    for path in &[
        "../escape.patch",
        "../../escape.patch",
        "patches/../../escape.patch",
        "sub/../../escape.patch",
    ] {
        let res = execute_tool(
            &canonical_root,
            "write_patch_file",
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
fn test_tool_write_patch_file_size_cap_and_params() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. Missing filename
    let res_no_file = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({"content": "diff"})),
    );
    assert_eq!(res_no_file.is_error, Some(true));

    // 2. Missing content
    let res_no_content = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({"filename": "test.patch"})),
    );
    assert_eq!(res_no_content.is_error, Some(true));

    // 3. Exceeds 1 MB cap
    let large_content = "a".repeat(1024 * 1024 + 1);
    let res_large = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "large.patch",
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
fn test_tool_write_patch_file_path_confinement_and_symlinks() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. External absolute path rejected
    let res_ext = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "/etc/passwd.patch",
            "content": "diff"
        })),
    );
    assert_eq!(res_ext.is_error, Some(true));

    // 2. Full absolute path inside patches/ accepted
    let full_abs = canonical_root.join("patches/abs.patch");
    let res_abs = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": full_abs.to_string_lossy(),
            "content": "diff abs"
        })),
    );
    assert!(res_abs.is_error.is_none());
    assert!(canonical_root.join("patches/abs.patch").is_file());

    // 3. Leading ./patches/ prefix accepted
    let res_dot = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "./patches/dot.patch",
            "content": "diff dot"
        })),
    );
    assert!(res_dot.is_error.is_none());
    assert!(canonical_root.join("patches/dot.patch").is_file());

    // 4. Trailing slash rejected
    let res_slash = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "dir.patch/",
            "content": "diff dir"
        })),
    );
    assert_eq!(res_slash.is_error, Some(true));

    // 5. Backslash rejected
    let res_bs = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": r"..\bs.patch",
            "content": "diff bs"
        })),
    );
    assert_eq!(res_bs.is_error, Some(true));

    // 6. Symlink rejection
    #[cfg(unix)]
    {
        let dangling = canonical_root.join("patches/sym_escape.patch");
        let outside = temp.join("outside.patch");
        std::os::unix::fs::symlink(&outside, &dangling).unwrap();
        let res_sym = execute_tool(
            &canonical_root,
            "write_patch_file",
            Some(&json!({
                "filename": "sym_escape.patch",
                "content": "diff symlink",
                "overwrite": true
            })),
        );
        assert_eq!(res_sym.is_error, Some(true));
    }
}

#[test]
#[cfg(unix)]
fn test_tool_write_patch_file_rejects_symlink_scope_dir() {
    let (_td, temp) = temp_workspace();
    fs::create_dir_all(temp.join("other_dir")).unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    // Create patches as a symlink to other_dir
    std::os::unix::fs::symlink(
        canonical_root.join("other_dir"),
        canonical_root.join("patches"),
    )
    .unwrap();

    let res = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "escape.patch",
            "content": "diff content"
        })),
    );
    assert_eq!(res.is_error, Some(true));
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("symlink"));
        }
    }
}
