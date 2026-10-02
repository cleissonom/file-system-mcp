mod workspace_support;

use serde_json::json;
use std::fs;
use workspace_support::{McpClient, call, failure, success, text};

#[test]
fn recursive_directory_creation_validates_the_whole_path_before_creating_parents() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".gitignore"), "blocked/\n").unwrap();
    let mut client = McpClient::spawn(root.path());
    for path in ["new/.git/objects", "new/blocked/child", "new/.env/child"] {
        failure(
            &mut client,
            "create_directory",
            json!({"path": path, "recursive": true}),
        );
        assert!(
            !root.path().join("new").exists(),
            "Created parents of rejected path {path}"
        );
    }
}

#[test]
fn scoped_writes_reject_protected_filenames_without_creating_parents() {
    let root = tempfile::tempdir().unwrap();
    let mut client = McpClient::spawn(root.path());
    for (tool, filename, directory) in [
        ("write_plan_file", "new/.env.md", "plans"),
        ("write_patch_file", "new/private.key.diff", "patches"),
    ] {
        failure(
            &mut client,
            tool,
            json!({"filename": filename, "content": "body"}),
        );
        assert!(!root.path().join(directory).exists());
    }
}

#[test]
fn nested_repositories_enforce_their_own_ignore_rules_and_workspace_rules() {
    let root = tempfile::tempdir().unwrap();
    for repository in ["repo-a", "repo-b"] {
        fs::create_dir_all(root.path().join(repository).join(".git/info")).unwrap();
        fs::write(
            root.path().join(repository).join("code.txt"),
            "visible needle",
        )
        .unwrap();
    }
    fs::write(root.path().join(".gitignore"), "global.txt\n").unwrap();
    fs::write(root.path().join("repo-a/.gitignore"), "/dist/\n*.tmp\n").unwrap();
    fs::write(root.path().join("repo-a/.git/info/exclude"), "local.txt\n").unwrap();
    fs::write(root.path().join("repo-b/.gitignore"), "/target/\n").unwrap();
    fs::create_dir(root.path().join("repo-a/dist")).unwrap();
    fs::create_dir(root.path().join("repo-b/target")).unwrap();
    for path in [
        "repo-a/dist/code.txt",
        "repo-a/cache.tmp",
        "repo-a/local.txt",
        "repo-a/global.txt",
        "repo-b/target/code.txt",
    ] {
        fs::write(root.path().join(path), "hidden needle").unwrap();
    }
    let mut client = McpClient::spawn(root.path());
    for path in ["repo-a/code.txt", "repo-b/code.txt"] {
        assert_eq!(
            success(&mut client, "file_info", json!({"path": path}))["type"],
            "file"
        );
    }
    for path in [
        "repo-a/dist/code.txt",
        "repo-a/cache.tmp",
        "repo-a/local.txt",
        "repo-a/global.txt",
        "repo-b/target/code.txt",
    ] {
        failure(&mut client, "read_file", json!({"path": path}));
        failure(
            &mut client,
            "write_file",
            json!({"path": path, "content": "replacement", "overwrite": true}),
        );
    }
    let listing = call(&mut client, "list_directory", json!({"depth": 3}));
    assert!(text(&listing).contains("repo-a/code.txt"));
    assert!(text(&listing).contains("repo-b/code.txt"));
    assert!(!text(&listing).contains("cache.tmp"));
    let search = call(&mut client, "search_files", json!({"query": "needle"}));
    assert!(text(&search).contains("visible needle"));
    assert!(!text(&search).contains("hidden needle"));
}

#[test]
fn ignore_rules_use_their_own_directory_and_preserve_negation_precedence() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir_all(root.path().join("repo/.git/info")).unwrap();
    fs::create_dir_all(root.path().join("repo/sub/cache")).unwrap();
    fs::create_dir_all(root.path().join("repo/blocked")).unwrap();
    fs::write(root.path().join(".gitignore"), "/repo/root-hidden.txt\n").unwrap();
    fs::write(root.path().join("repo/.git/info/exclude"), "allowed.txt\n").unwrap();
    fs::write(
        root.path().join("repo/.gitignore"),
        "/blocked/\n!/blocked/child.txt\n*.tmp\n!allowed.txt\n",
    )
    .unwrap();
    fs::write(
        root.path().join("repo/sub/.gitignore"),
        "/hidden.txt\ncache/private.txt\n!keep.tmp\n",
    )
    .unwrap();
    for path in [
        "repo/root-hidden.txt",
        "repo/sub/hidden.txt",
        "repo/sub/cache/private.txt",
        "repo/blocked/child.txt",
        "repo/sub/keep.tmp",
        "repo/allowed.txt",
        "repo/hidden.txt",
    ] {
        fs::write(root.path().join(path), format!("needle {path}")).unwrap();
    }
    let mut client = McpClient::spawn(root.path());
    for path in [
        "repo/sub/hidden.txt",
        "repo/sub/cache/private.txt",
        "repo/root-hidden.txt",
        "repo/blocked/child.txt",
    ] {
        failure(&mut client, "file_info", json!({"path": path}));
        failure(&mut client, "read_file", json!({"path": path}));
        failure(
            &mut client,
            "write_file",
            json!({"path": path, "content": "replacement", "overwrite": true}),
        );
    }
    for path in ["repo/sub/keep.tmp", "repo/allowed.txt", "repo/hidden.txt"] {
        success(&mut client, "file_info", json!({"path": path}));
    }
    let result = call(&mut client, "search_files", json!({"query": "needle"}));
    assert!(text(&result).contains("repo/sub/keep.tmp"));
    assert!(text(&result).contains("repo/allowed.txt"));
    assert!(!text(&result).contains("repo/sub/hidden.txt"));
    assert!(!text(&result).contains("private.txt"));
    assert!(!text(&result).contains("root-hidden.txt"));
    assert!(!text(&result).contains("blocked/child.txt"));
}

#[test]
fn ignore_files_with_a_utf8_bom_still_protect_the_first_pattern() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".gitignore"), "\u{feff}hidden.txt\n").unwrap();
    fs::write(root.path().join("hidden.txt"), "protected").unwrap();
    let mut client = McpClient::spawn(root.path());
    failure(&mut client, "read_file", json!({"path": "hidden.txt"}));
    failure(
        &mut client,
        "write_file",
        json!({"path": "hidden.txt", "content": "replacement", "overwrite": true}),
    );
    assert_eq!(
        fs::read_to_string(root.path().join("hidden.txt")).unwrap(),
        "protected"
    );
}

#[test]
fn alternate_filesystem_spellings_cannot_bypass_ignored_existing_paths() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join(".gitignore"),
        "hidden.txt\n!HIDDEN.TXT\ncafé.txt\nblocked/\n",
    )
    .unwrap();
    fs::write(root.path().join("hidden.txt"), "protected").unwrap();
    fs::write(root.path().join("café.txt"), "protected").unwrap();
    fs::create_dir(root.path().join("blocked")).unwrap();
    fs::write(root.path().join("source.txt"), "source").unwrap();
    let mut client = McpClient::spawn(root.path());
    for path in ["HIDDEN.TXT", "cafe\u{301}.txt"] {
        if root.path().join(path).exists() {
            failure(&mut client, "read_file", json!({"path": path}));
            failure(&mut client, "file_info", json!({"path": path}));
            failure(
                &mut client,
                "write_file",
                json!({"path": path, "content": "replacement", "overwrite": true}),
            );
            failure(&mut client, "delete_path", json!({"path": path}));
        } else {
            success(
                &mut client,
                "write_file",
                json!({"path": path, "content": "distinct file"}),
            );
            assert_eq!(
                fs::read_to_string(root.path().join(path)).unwrap(),
                "distinct file"
            );
        }
    }
    if root.path().join("BLOCKED").exists() {
        failure(
            &mut client,
            "write_file",
            json!({"path": "BLOCKED/new.txt", "content": "body"}),
        );
        failure(
            &mut client,
            "copy_path",
            json!({"source": "source.txt", "destination": "BLOCKED/new.txt"}),
        );
        failure(
            &mut client,
            "move_path",
            json!({"source": "source.txt", "destination": "BLOCKED/new.txt"}),
        );
    }
    assert_eq!(
        fs::read_to_string(root.path().join("hidden.txt")).unwrap(),
        "protected"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("café.txt")).unwrap(),
        "protected"
    );
    assert!(root.path().join("source.txt").exists());
    assert!(!root.path().join("blocked/new.txt").exists());
}

#[test]
fn ignored_legacy_scopes_do_not_prevent_other_workspace_tools_from_starting() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(".gitignore"), "plans/\npatches/\n").unwrap();
    fs::write(root.path().join("readme.txt"), "readable").unwrap();
    let mut client = McpClient::spawn(root.path());
    assert_eq!(
        success(&mut client, "workspace_info", json!({}))["writable"],
        true
    );
    let result = call(&mut client, "read_file", json!({"path": "readme.txt"}));
    assert_eq!(text(&result), "readable");
    failure(
        &mut client,
        "write_plan_file",
        json!({"filename": "plan.md", "content": "body"}),
    );
    failure(
        &mut client,
        "write_patch_file",
        json!({"filename": "change.patch", "content": "diff"}),
    );
    assert!(!root.path().join("plans").exists());
    assert!(!root.path().join("patches").exists());
}

#[test]
fn search_handles_long_unicode_lines_without_crashing_the_server() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("unicode.txt"),
        format!("needle {}\n", "界".repeat(200)),
    )
    .unwrap();
    let mut client = McpClient::spawn(root.path());
    let result = call(&mut client, "search_files", json!({"query": "needle"}));
    assert_ne!(result["isError"], true);
    assert!(text(&result).contains("needle"));
    let ping = client.send_request(json!({"id": 2, "method": "ping"}));
    assert_eq!(ping["result"], json!({}));
}
