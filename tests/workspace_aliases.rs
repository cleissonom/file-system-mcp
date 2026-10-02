mod workspace_support;

use serde_json::json;
use std::fs;
use workspace_support::{McpClient, failure, success};

fn modification(path: &str, after: &str) -> String {
    format!("--- a/{path}\n+++ b/{path}\n@@ -1 +1 @@\n-before\n+{after}\n")
}

#[test]
fn patch_alias_collisions_are_rejected_before_any_file_is_changed() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("file.txt"), "before\n").unwrap();
    let aliases = root.path().join("FILE.TXT").exists();
    if !aliases {
        fs::write(root.path().join("FILE.TXT"), "before\n").unwrap();
    }
    let patch = modification("file.txt", "first") + &modification("FILE.TXT", "second");
    let mut client = McpClient::spawn(root.path());
    if aliases {
        failure(&mut client, "apply_patch", json!({"patch_content": patch}));
        assert_eq!(
            fs::read_to_string(root.path().join("file.txt")).unwrap(),
            "before\n"
        );
    } else {
        success(&mut client, "apply_patch", json!({"patch_content": patch}));
        assert_eq!(
            fs::read_to_string(root.path().join("file.txt")).unwrap(),
            "first\n"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("FILE.TXT")).unwrap(),
            "second\n"
        );
    }
}

#[test]
fn distinct_hard_link_names_remain_independent_patch_targets() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("first.txt"), "before\n").unwrap();
    fs::hard_link(
        root.path().join("first.txt"),
        root.path().join("second.txt"),
    )
    .unwrap();
    let patch = modification("first.txt", "first") + &modification("second.txt", "second");
    let mut client = McpClient::spawn(root.path());
    success(&mut client, "apply_patch", json!({"patch_content": patch}));
    assert_eq!(
        fs::read_to_string(root.path().join("first.txt")).unwrap(),
        "first\n"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("second.txt")).unwrap(),
        "second\n"
    );
}

#[test]
fn moves_between_hard_links_reject_a_successful_os_noop() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("source.txt"), "body").unwrap();
    fs::hard_link(
        root.path().join("source.txt"),
        root.path().join("destination.txt"),
    )
    .unwrap();
    let mut client = McpClient::spawn(root.path());
    let args = json!({"source": "source.txt", "destination": "destination.txt", "overwrite": true});
    failure(&mut client, "move_path", args.clone());
    assert_eq!(
        fs::read_to_string(root.path().join("source.txt")).unwrap(),
        "body"
    );
    assert_eq!(
        fs::read_to_string(root.path().join("destination.txt")).unwrap(),
        "body"
    );
    success(&mut client, "copy_path", args);
    assert!(root.path().join("source.txt").exists());
}

#[test]
fn transfers_reject_alias_identity_and_directory_containment() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("tree")).unwrap();
    fs::write(root.path().join("tree/file.txt"), "body").unwrap();
    let aliases = root.path().join("TREE").exists();
    let mut client = McpClient::spawn(root.path());
    let source = "tree/file.txt";
    let destination = if aliases { "TREE/FILE.TXT" } else { source };
    for tool in ["copy_path", "move_path"] {
        failure(
            &mut client,
            tool,
            json!({"source": source, "destination": destination, "overwrite": true}),
        );
        let destination = if aliases { "TREE/copy" } else { "tree/copy" };
        failure(
            &mut client,
            tool,
            json!({"source": "tree", "destination": destination, "recursive": true}),
        );
    }
    assert_eq!(
        fs::read_to_string(root.path().join(source)).unwrap(),
        "body"
    );
    assert!(!root.path().join("tree/copy").exists());
}
