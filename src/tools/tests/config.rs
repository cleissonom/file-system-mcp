use super::*;

#[test]
fn test_execute_tool_with_custom_plans_and_patches_dirs() {
    let (_td, temp) = temp_workspace();
    let root = temp.canonicalize().unwrap();
    let config = ServerConfig::new(
        root.clone(),
        root.join("my_special_plans"),
        root.join("my_special_patches"),
    )
    .unwrap();

    // 1. Write plan
    let res_plan = execute_tool_with_config(
        &config,
        "write_plan_file",
        Some(&json!({
            "filename": "custom_plan.md",
            "content": "# Custom Plan Body"
        })),
    );
    assert!(res_plan.is_error.is_none());
    assert!(root.join("my_special_plans/custom_plan.md").is_file());

    // 2. Write patch
    fs::write(root.join("code.txt"), "val_before\n").unwrap();
    let res_patch = execute_tool_with_config(
        &config,
        "write_patch_file",
        Some(&json!({
            "filename": "custom_patch.patch",
            "content": "--- a/code.txt\n+++ b/code.txt\n@@ -1 +1 @@\n-val_before\n+val_after\n"
        })),
    );
    assert!(res_patch.is_error.is_none());
    assert!(root.join("my_special_patches/custom_patch.patch").is_file());

    // 3. Read back plan
    let res_read = execute_tool_with_config(
        &config,
        "read_file",
        Some(&json!({"path": "my_special_plans/custom_plan.md"})),
    );
    assert!(res_read.is_error.is_none());

    // 4. Validate patch
    let res_val = execute_tool_with_config(
        &config,
        "validate_patch",
        Some(&json!({
            "patch_filename": "custom_patch.patch",
            "target_dir": "."
        })),
    );
    assert!(res_val.is_error.is_none());

    // All configured scopes share the workspace boundary.
    let ext_dir = tempfile::tempdir().unwrap();
    assert!(
        ServerConfig::new(
            root.clone(),
            ext_dir.path().to_path_buf(),
            root.join("patches")
        )
        .is_err()
    );
}
