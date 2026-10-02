use super::{ChangeKind, MAX_PATCH_BYTES, MAX_PATCH_FILES, parse};
use std::path::Path;

const MODIFY: &str = "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-before\n+after\n";

#[test]
fn multiple_files_keep_their_operation_and_apply_independently() {
    let content = format!(
        "{MODIFY}--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+new\n\
         --- a/old.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-old\n"
    );
    let changes = parse(&content).unwrap();
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].kind, ChangeKind::Modify);
    assert_eq!(changes[1].kind, ChangeKind::Create);
    assert_eq!(changes[2].kind, ChangeKind::Delete);
    assert_eq!(changes[0].apply_to("before\n").unwrap(), "after\n");
    assert_eq!(changes[1].apply_to("").unwrap(), "new\n");
    assert_eq!(changes[2].apply_to("old\n").unwrap(), "");
}

#[test]
fn normal_git_framing_and_regular_file_creation_metadata_are_supported() {
    let content = concat!(
        "diff --git a/file.txt b/file.txt\nindex 123abc..456def 100644\n",
        "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-before\n+after\n",
        "diff --git a/new.txt b/new.txt\nnew file mode 100755\n",
        "index 0000000..123abcd\n--- /dev/null\n+++ b/new.txt\n",
        "@@ -0,0 +1 @@\n+new\n"
    );
    let changes = parse(content).unwrap();
    assert_eq!(changes[0].apply_to("before\n").unwrap(), "after\n");
    assert_eq!(changes[1].kind, ChangeKind::Create);
}

#[test]
fn git_deletion_metadata_is_supported() {
    let content = concat!(
        "diff --git a/old.txt b/old.txt\ndeleted file mode 100644\n",
        "index 123abcd..0000000\n--- a/old.txt\n+++ /dev/null\n",
        "@@ -1 +0,0 @@\n-old\n"
    );
    let changes = parse(content).unwrap();
    assert_eq!(changes[0].kind, ChangeKind::Delete);
    assert_eq!(changes[0].apply_to("old\n").unwrap(), "");
}

#[test]
fn unicode_and_missing_final_newlines_are_preserved() {
    let content = concat!(
        "--- a/café.txt\n+++ b/café.txt\n@@ -1 +1 @@\n",
        "-olá 🦀\n\\ No newline at end of file\n",
        "+こんにちは 🌻\n\\ No newline at end of file\n"
    );
    let change = parse(content).unwrap().remove(0);
    assert_eq!(change.path, Path::new("café.txt"));
    assert_eq!(change.apply_to("olá 🦀").unwrap(), "こんにちは 🌻");
}

#[test]
fn content_that_looks_like_file_headers_is_not_split_into_another_patch() {
    let content = concat!(
        "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n",
        "--- a/content.txt\n+++ b/content.txt\n"
    );
    let changes = parse(content).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        changes[0].apply_to("-- a/content.txt\n").unwrap(),
        "++ b/content.txt\n"
    );
}

#[test]
fn adjacent_hunks_change_only_their_matching_lines() {
    let content = concat!(
        "--- file.txt\n+++ file.txt\n@@ -1 +1 @@\n-first\n+one\n",
        "@@ -3 +3 @@\n-third\n+three\n"
    );
    let change = parse(content).unwrap().remove(0);
    assert_eq!(
        change.apply_to("first\nsecond\nthird\n").unwrap(),
        "one\nsecond\nthree\n"
    );
}

#[test]
fn relative_filenames_and_tab_separated_timestamps_are_supported() {
    let content = concat!(
        "--- directory/my file.txt\t2026-01-01 00:00:00\n",
        "+++ directory/my file.txt\t2026-01-02 00:00:00\n",
        "@@ -1 +1 @@\n-before\n+after\n"
    );
    let change = parse(content).unwrap().remove(0);
    assert_eq!(change.path, Path::new("directory/my file.txt"));
    assert_eq!(change.apply_to("before\n").unwrap(), "after\n");
}

#[test]
fn stale_content_is_rejected() {
    let change = parse(MODIFY).unwrap().remove(0);
    assert!(change.apply_to("changed by someone else\n").is_err());
}

#[test]
fn creation_cannot_replace_existing_content() {
    let change = parse("--- /dev/null\n+++ new.txt\n@@ -0,0 +1 @@\n+new\n")
        .unwrap()
        .remove(0);
    assert!(change.apply_to("existing\n").is_err());
}

#[test]
fn deletion_cannot_discard_unmentioned_lines() {
    let change = parse("--- old.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-old\n")
        .unwrap()
        .remove(0);
    assert!(change.apply_to("old\nretained\n").is_err());
}

#[test]
fn untrusted_paths_cannot_escape_or_change_interpretation() {
    for path in [
        "../outside",
        "nested/../../outside",
        "/tmp/outside",
        "a//tmp/outside",
        "b//tmp/outside",
        "a/../outside",
        "C:/outside",
        "file\\name",
        "\"quoted\"",
        "",
        ".",
        "nested/./file",
        "nested//file",
        "trailing/",
        "nul\0file",
    ] {
        let content = format!("--- {path}\n+++ {path}\n@@ -1 +1 @@\n-old\n+new\n");
        assert!(parse(&content).is_err(), "accepted unsafe path: {path:?}");
    }
}

#[test]
fn filenames_are_not_trimmed_or_reinterpreted() {
    let content = "--- a/ leading.txt\n+++ b/ leading.txt\n@@ -1 +1 @@\n-old\n+new\n";
    let change = parse(content).unwrap().remove(0);
    assert_eq!(change.path, Path::new(" leading.txt"));
}

#[test]
fn mismatched_and_duplicate_targets_are_rejected() {
    assert!(parse("--- a/old.txt\n+++ b/new.txt\n@@ -1 +1 @@\n-old\n+new\n").is_err());
    assert!(parse(&format!("{MODIFY}{MODIFY}")).is_err());
    assert!(parse("--- /dev/null\n+++ /dev/null\n@@ -0,0 +0,0 @@\n").is_err());
}

#[test]
fn unsupported_git_metadata_is_rejected_before_application() {
    for metadata in [
        "old mode 100644\nnew mode 100755\n",
        "rename from file.txt\n",
        "rename to file.txt\n",
        "copy from file.txt\n",
        "copy to file.txt\n",
        "similarity index 100%\n",
        "new file mode 120000\n",
        "index abc..def 120000\n",
        "GIT binary patch\n",
        "Binary files a/file.txt and b/file.txt differ\n",
        "index not-an-object-id\n",
        "new file mode 100644\n",
    ] {
        let content = format!("diff --git a/file.txt b/file.txt\n{metadata}{MODIFY}");
        assert!(
            parse(&content).is_err(),
            "accepted unsupported metadata: {metadata:?}"
        );
    }
}

#[test]
fn git_framing_must_agree_with_the_file_headers() {
    let content = format!("diff --git a/other.txt b/other.txt\n{MODIFY}");
    assert!(parse(&content).is_err());
    let content = format!("diff --git \"a/file.txt\" \"b/file.txt\"\n{MODIFY}");
    assert!(parse(&content).is_err());
}

#[test]
fn invalid_input_and_missing_or_malformed_hunks_are_rejected() {
    for content in [
        "",
        "\n",
        "not a patch",
        "--- file\n+++ file\n",
        "@@ -1 +1 @@\n-old\n+new\n",
        "--- file\n@@ -1 +1 @@\n-old\n+new\n",
        "--- file\n+++ file\n@@ -1,2 +1 @@\n-old\n+new\n",
        "--- file\n+++ file\n@@ -1 +1 @@\n-old\n+new\ntrailing garbage\n",
        "--- file\n+++ file\n@@ -0 +1 @@\n-old\n+new\n",
        "--- file\n+++ file\n@@ -18446744073709551615,2 +1 @@\n-old\n+new\n",
    ] {
        assert!(
            parse(content).is_err(),
            "accepted invalid patch: {content:?}"
        );
    }
}

#[test]
fn later_malformed_files_reject_the_entire_patch() {
    let content = format!("{MODIFY}--- a/other.txt\n+++ b/other.txt\n@@ -1,2 +1 @@\n-old\n+new\n");
    assert!(parse(&content).is_err());
}

#[test]
fn resource_limits_reject_oversized_and_excessive_file_sets() {
    assert!(parse(&"x".repeat(MAX_PATCH_BYTES + 1)).is_err());
    let content: String = (0..=MAX_PATCH_FILES)
        .map(|index| format!("--- a/{index}.txt\n+++ b/{index}.txt\n@@ -1 +1 @@\n-old\n+new\n"))
        .collect();
    assert!(parse(&content).is_err());
}
