use rustix::fs::{FlockOperation, OFlags, flock};
use std::fs::{self, DirBuilder, File, Metadata, OpenOptions};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub(super) struct PreparedDatabase {
    pub path: PathBuf,
    pub database: File,
    pub lock: File,
}

pub(super) fn prepare(path: &Path) -> io::Result<PreparedDatabase> {
    let path = validated_path(path)?;
    let lock_path = sidecar_path(&path, ".lock");
    let lock = open_private_file(&lock_path)?;
    validate_file(&lock.metadata()?)?;
    lock_database(&lock)?;
    verify_identity(&lock_path, &lock)?;
    let file = open_private_file(&path)?;
    validate_file(&file.metadata()?)?;
    verify_identity(&path, &file)?;
    Ok(PreparedDatabase {
        path,
        database: file,
        lock,
    })
}

fn validated_path(path: &Path) -> io::Result<PathBuf> {
    let filename = path
        .file_name()
        .ok_or_else(|| io::Error::other("Observer database path must name a file"))?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    create_private_parents(parent)?;
    let parent = parent.canonicalize()?;
    validate_parent(&parent)?;
    let path = parent.join(filename);
    verify_existing_file(&path)?;
    verify_sidecars(&path)?;
    Ok(path)
}

fn open_private_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags((OFlags::NOFOLLOW | OFlags::NONBLOCK).bits() as i32)
        .open(path)
}

fn verify_existing_file(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_file(&metadata),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn create_private_parents(path: &Path) -> io::Result<()> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => return Ok(()),
        Ok(_) => {
            return Err(io::Error::other(
                "Observer database parent must be a directory",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        create_private_parents(parent)?;
    }
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => validate_parent(path),
        Err(error) => Err(error),
    }
}

fn validate_parent(path: &Path) -> io::Result<()> {
    let metadata = fs::metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o022 != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Observer database directory must be owned by the current user and not writable by other users",
        ));
    }
    Ok(())
}

fn validate_file(metadata: &Metadata) -> io::Result<()> {
    if !metadata.is_file()
        || metadata.uid() != rustix::process::geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o600
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Observer database and sidecar files must be regular files owned by the current user with permissions 0600",
        ));
    }
    if metadata.nlink() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Observer database and sidecar files must not have hardlinks; choose a unique private database file",
        ));
    }
    Ok(())
}

fn lock_database(file: &File) -> io::Result<()> {
    flock(file, FlockOperation::NonBlockingLockExclusive).map_err(|error| {
        let error: io::Error = error.into();
        if error.kind() == io::ErrorKind::WouldBlock {
            return io::Error::new(io::ErrorKind::WouldBlock,
                "Observer database is already in use by another MCP observer; stop it or choose a different --dashboard-db");
        }
        error
    })
}

pub(super) fn verify_identity(path: &Path, file: &File) -> io::Result<()> {
    let current = fs::symlink_metadata(path)?;
    validate_file(&current)?;
    let opened = file.metadata()?;
    if current.dev() != opened.dev() || current.ino() != opened.ino() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Observer database changed during startup; choose a private database directory",
        ));
    }
    Ok(())
}

pub(super) fn verify_sidecars(path: &Path) -> io::Result<()> {
    for suffix in ["-wal", "-shm", "-journal", ".lock"] {
        match fs::symlink_metadata(sidecar_path(path, suffix)) {
            Ok(metadata) => validate_file(&metadata)?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}
