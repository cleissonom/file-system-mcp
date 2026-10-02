use super::*;

#[test]
fn test_tool_validate_patch_clean_and_failing() {
    let (_td, temp) = temp_workspace();
    let svc_dir = temp.join("services/auth-service");
    fs::create_dir_all(&svc_dir).unwrap();
    let target_file = svc_dir.join("handler.go");
    fs::write(&target_file, "package auth\nfunc Handle() {}\n").unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. Write clean patch
    let clean_diff = "--- a/handler.go\n+++ b/handler.go\n@@ -1,2 +1,3 @@\n package auth\n+func NewHelper() {}\n func Handle() {}\n";
    let res_write = execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "clean.patch",
            "content": clean_diff
        })),
    );
    assert!(res_write.is_error.is_none());

    // 2. Validate clean patch (dry-run only!)
    let res_val = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "clean.patch",
            "target_dir": "services/auth-service"
        })),
    );
    assert!(res_val.is_error.is_none());
    match &res_val.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("applies cleanly"));
            assert!(text.contains("dry-run check passed"));
        }
    }
    // SAFETY INVARIANT: Working tree must NOT be modified
    assert_eq!(
        fs::read_to_string(&target_file).unwrap(),
        "package auth\nfunc Handle() {}\n"
    );

    // 3. Write failing/conflicting patch
    let conflict_diff =
        "--- a/handler.go\n+++ b/handler.go\n@@ -1,2 +1,2 @@\n-nonexistent line\n+replaced\n";
    execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "conflict.patch",
            "content": conflict_diff
        })),
    );

    // 4. Validate failing patch
    let res_fail = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "conflict.patch",
            "target_dir": "services/auth-service"
        })),
    );
    assert_eq!(res_fail.is_error, Some(true));
    match &res_fail.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("failed to apply"));
        }
    }
    // Target file must remain intact
    assert_eq!(
        fs::read_to_string(&target_file).unwrap(),
        "package auth\nfunc Handle() {}\n"
    );

    // 5. Non-existent patch
    let res_missing = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "nonexistent.patch",
            "target_dir": "services/auth-service"
        })),
    );
    assert_eq!(res_missing.is_error, Some(true));
    match &res_missing.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("does not exist"));
        }
    }
}

#[test]
fn test_tool_validate_patch_security_and_traversal() {
    let (_td, temp) = temp_workspace();
    fs::create_dir_all(temp.join("sub")).unwrap();
    fs::write(temp.join("sub/file.txt"), "hello\n").unwrap();
    fs::write(temp.join(".gitignore"), "ignored_dir/\n").unwrap();
    fs::create_dir_all(temp.join("ignored_dir")).unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    // Write a valid patch
    execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "sec.patch",
            "content": "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-hello\n+world\n"
        })),
    );

    // 1. Directory traversal in target_dir
    let res_trav = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "sec.patch",
            "target_dir": "../outside"
        })),
    );
    assert_eq!(res_trav.is_error, Some(true));

    // 2. Denylisted target_dir (.git)
    fs::create_dir_all(canonical_root.join(".git")).unwrap();
    let res_deny = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "sec.patch",
            "target_dir": ".git"
        })),
    );
    assert_eq!(res_deny.is_error, Some(true));

    // 3. Target dir is a file
    let res_file = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "sec.patch",
            "target_dir": "sub/file.txt"
        })),
    );
    assert_eq!(res_file.is_error, Some(true));

    // 4. Gitignored target dir
    let res_ign = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "sec.patch",
            "target_dir": "ignored_dir"
        })),
    );
    assert_eq!(res_ign.is_error, Some(true));

    // 5. Directory traversal in patch_filename
    let res_ptrav = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "../escape.patch",
            "target_dir": "sub"
        })),
    );
    assert_eq!(res_ptrav.is_error, Some(true));

    // 6. Missing required patch_filename
    let res_noparam = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "target_dir": "sub"
        })),
    );
    assert_eq!(res_noparam.is_error, Some(true));
}

#[test]
fn test_tool_validate_patch_does_not_create_dirs_on_missing_patch() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    // 1. Calling validate_patch on non-existent patches/ directory and subpath
    let res = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "nonexistent_subdir/missing.patch",
            "target_dir": "."
        })),
    );
    assert_eq!(res.is_error, Some(true));
    // Invariant: dry-run validate tool MUST NOT create directories on disk
    assert!(
        !canonical_root
            .join("patches")
            .join("nonexistent_subdir")
            .exists()
    );
}

#[test]
fn test_tool_validate_patch_rejects_denylisted_target_files_in_patch() {
    let (_td, temp) = temp_workspace();
    let canonical_root = temp.canonicalize().unwrap();

    // Target file .env exists in workspace
    let env_file = canonical_root.join(".env");
    fs::write(&env_file, "DATABASE_URL=secret\n").unwrap();

    // 1. Patch file targeting .env written to patches/
    let env_patch =
        "--- a/.env\n+++ b/.env\n@@ -1 +1 @@\n-DATABASE_URL=secret\n+DATABASE_URL=hacked\n";
    execute_tool(
        &canonical_root,
        "write_patch_file",
        Some(&json!({
            "filename": "env_probe.patch",
            "content": env_patch
        })),
    );

    // 2. Validate patch must reject targeting .env to prevent patch oracle attacks
    let res = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "env_probe.patch",
            "target_dir": "."
        })),
    );
    assert_eq!(res.is_error, Some(true));
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(
                text.contains("denylist") || text.contains("denied"),
                "Expected security denylist rejection but got: {}",
                text
            );
        }
    }
}

#[test]
fn test_tool_validate_patch_inline_patch_content() {
    let (_td, temp) = temp_workspace();
    let file_path = temp.join("file.txt");
    fs::write(&file_path, "hello\n").unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    let clean_diff = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-hello\n+world\n";
    let res = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_content": clean_diff,
            "target_dir": "."
        })),
    );
    assert!(res.is_error.is_none());
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("applies cleanly"));
            assert!(text.contains("dry-run check passed"));
        }
    }
    // File must remain unchanged
    assert_eq!(fs::read_to_string(&file_path).unwrap(), "hello\n");
}

#[test]
fn test_tool_validate_patch_oversized() {
    let (_td, temp) = temp_workspace();
    fs::create_dir_all(temp.join("patches")).unwrap();
    let canonical_root = temp.canonicalize().unwrap();

    // Create a patch file on disk that exceeds 1 MB
    let huge_path = canonical_root.join("patches/huge.patch");
    let huge_file = fs::File::create(&huge_path).unwrap();
    huge_file.set_len((1024 * 1024 + 10) as u64).unwrap();

    let res = execute_tool(
        &canonical_root,
        "validate_patch",
        Some(&json!({
            "patch_filename": "huge.patch",
            "target_dir": "."
        })),
    );
    assert_eq!(res.is_error, Some(true));
    match &res.content[0] {
        crate::protocol::ToolContent::Text { text } => {
            assert!(text.contains("exceeds safety limit"));
        }
    }
}
