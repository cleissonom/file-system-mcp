use super::*;
use std::io::Read;
use std::os::unix::fs::symlink;

#[test]
fn opened_parent_handles_do_not_follow_replacement_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("parent")).unwrap();
    std::fs::write(root.path().join("parent/file.txt"), "inside").unwrap();
    std::fs::write(external.path().join("file.txt"), "outside").unwrap();
    let workspace = Workspace::open(root.path()).unwrap();
    let entry = workspace.entry(Path::new("parent/file.txt")).unwrap();
    std::fs::rename(root.path().join("parent"), root.path().join("moved")).unwrap();
    symlink(external.path(), root.path().join("parent")).unwrap();
    let mut content = String::new();
    entry
        .open_file()
        .unwrap()
        .read_to_string(&mut content)
        .unwrap();
    assert_eq!(content, "inside");
    assert!(workspace.open_file(Path::new("parent/file.txt")).is_err());
}

#[test]
fn final_file_symlink_replacement_is_rejected_after_parent_resolution() {
    let root = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("file.txt"), "inside").unwrap();
    std::fs::write(external.path().join("file.txt"), "outside").unwrap();
    let workspace = Workspace::open(root.path()).unwrap();
    let entry = workspace.entry(Path::new("file.txt")).unwrap();
    std::fs::remove_file(root.path().join("file.txt")).unwrap();
    symlink(
        external.path().join("file.txt"),
        root.path().join("file.txt"),
    )
    .unwrap();
    assert!(entry.open_file().is_err());
    assert!(entry.metadata().is_err());
}
