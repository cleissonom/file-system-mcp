use super::*;
use std::os::unix::fs::PermissionsExt;

#[test]
fn directory_copy_limits_actual_bytes_when_sources_grow_after_inventory() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("source")).unwrap();
    for name in ["first.txt", "second.txt"] {
        std::fs::write(root.path().join("source").join(name), "").unwrap();
    }
    let workspace = Workspace::open(root.path()).unwrap();
    let entries = workspace.inventory(Path::new("source")).unwrap();
    for name in ["first.txt", "second.txt"] {
        std::fs::File::options()
            .write(true)
            .open(root.path().join("source").join(name))
            .unwrap()
            .set_len((MAX_COPY_BYTES / 2 + 1) as u64)
            .unwrap();
    }
    workspace
        .validate_copy_destination(Path::new("source"), Path::new("copy"), &entries)
        .unwrap();
    let destination = workspace.entry(Path::new("copy")).unwrap();
    let result = workspace.copy_directory(Path::new("source"), &destination, &entries);
    assert!(result.unwrap_err().contains("safety limit"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}

#[test]
fn failed_directory_publication_cleans_up_read_only_staging_directories() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("source/sub")).unwrap();
    std::fs::write(root.path().join("source/sub/file.txt"), "body").unwrap();
    for path in ["source", "source/sub"] {
        std::fs::set_permissions(
            root.path().join(path),
            std::fs::Permissions::from_mode(0o555),
        )
        .unwrap();
    }
    let workspace = Workspace::open(root.path()).unwrap();
    let entries = workspace.inventory(Path::new("source")).unwrap();
    let destination = workspace.entry(Path::new("copy")).unwrap();
    std::fs::create_dir(root.path().join("copy")).unwrap();
    let result = workspace.copy_directory(Path::new("source"), &destination, &entries);
    assert!(result.unwrap_err().contains("publish"));
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    for path in ["source", "source/sub"] {
        assert_eq!(
            std::fs::metadata(root.path().join(path))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o555
        );
        std::fs::set_permissions(
            root.path().join(path),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
    }
}
