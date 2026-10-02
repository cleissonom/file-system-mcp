use std::fs;
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub(super) fn socket_path() -> io::Result<PathBuf> {
    let uid = rustix::process::geteuid().as_raw();
    // /tmp keeps Unix socket paths below platform length limits even for long project paths.
    let directory = PathBuf::from(format!("/tmp/file-system-mcp-{uid}-runtime"));
    ensure_private_directory(&directory, uid)?;
    Ok(directory.join(format!("tunnel-health-{}.sock", std::process::id())))
}

fn ensure_private_directory(path: &Path, uid: u32) -> io::Result<()> {
    match fs::DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error),
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.uid() != uid || metadata.permissions().mode() & 0o777 != 0o700
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Tunnel health runtime directory must be an owned directory with permissions 0700",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_directory_is_created_and_reused() {
        let temporary = tempfile::tempdir().unwrap();
        let directory = temporary.path().join("runtime");
        let uid = rustix::process::geteuid().as_raw();
        ensure_private_directory(&directory, uid).unwrap();
        ensure_private_directory(&directory, uid).unwrap();
        assert_eq!(
            fs::metadata(directory).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn unsafe_existing_runtime_paths_are_rejected() {
        let temporary = tempfile::tempdir().unwrap();
        let uid = rustix::process::geteuid().as_raw();
        let file = temporary.path().join("file");
        fs::write(&file, "fixture").unwrap();
        let link = temporary.path().join("link");
        std::os::unix::fs::symlink(temporary.path(), &link).unwrap();
        let readable = temporary.path().join("readable");
        fs::create_dir(&readable).unwrap();
        fs::set_permissions(&readable, fs::Permissions::from_mode(0o755)).unwrap();
        for path in [file, link, readable] {
            assert_eq!(
                ensure_private_directory(&path, uid).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied
            );
        }
        assert_eq!(
            ensure_private_directory(temporary.path(), uid.saturating_add(1))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
    }
}
