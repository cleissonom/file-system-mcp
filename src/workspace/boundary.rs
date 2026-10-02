use super::Workspace;
use rustix::fd::{AsFd, OwnedFd};
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, Stat};
use std::ffi::OsString;
use std::fs::{File, Metadata};
use std::path::{Component, Path, PathBuf};

pub(super) struct Entry {
    pub parent: OwnedFd,
    pub name: OsString,
    pub path: PathBuf,
}

pub(super) fn duplicate(fd: &OwnedFd) -> Result<OwnedFd, String> {
    rustix::io::dup(fd).map_err(|e| e.to_string())
}

pub(super) fn open_directory(fd: impl AsFd, name: &Path) -> Result<OwnedFd, String> {
    fs::openat(
        fd,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| format!("Cannot open directory without symlinks: {e}"))
}

pub(super) fn relative(raw: &str) -> Result<PathBuf, String> {
    if raw.trim().is_empty() || raw.contains(['\0', '\\']) {
        return Err("Path cannot be empty or contain NUL/backslash characters".into());
    }
    validate_relative(Path::new(raw))
}

pub(super) fn validate_relative(path: &Path) -> Result<PathBuf, String> {
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(name) => relative.push(name),
            Component::CurDir => {}
            Component::ParentDir => return Err("Directory traversal detected (use of '..')".into()),
            _ => return Err("Path must be relative to the workspace root".into()),
        }
    }
    Ok(relative)
}

pub(super) fn require_child(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err("The workspace root cannot be mutated".into());
    }
    Ok(())
}

impl Workspace {
    pub(super) fn entry(&self, path: &Path) -> Result<Entry, String> {
        let path = validate_relative(path)?;
        require_child(&path)?;
        self.check_policy(&path, false)?;
        let parent = self.directory(path.parent().unwrap_or(Path::new("")))?;
        let entry = Entry {
            parent,
            name: path.file_name().unwrap().to_os_string(),
            path,
        };
        if let Some(stat) = entry.stat()? {
            self.check_policy(
                &entry.path,
                FileType::from_raw_mode(stat.st_mode) == FileType::Directory,
            )?;
        }
        Ok(entry)
    }

    pub(super) fn directory(&self, path: &Path) -> Result<OwnedFd, String> {
        let path = validate_relative(path)?;
        let mut fd = duplicate(&self.fd)?;
        let mut prefix = PathBuf::new();
        for component in path.components() {
            prefix.push(component);
            self.check_policy(&prefix, true)?;
            fd = open_directory(&fd, Path::new(component.as_os_str()))?;
        }
        Ok(fd)
    }

    pub(super) fn check_existing_ancestors(&self, path: &Path) -> Result<(), String> {
        let mut fd = duplicate(&self.fd)?;
        for component in path.components() {
            match fs::openat(
                &fd,
                component.as_os_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            ) {
                Ok(next) => fd = next,
                Err(rustix::io::Errno::NOENT) => return Ok(()),
                Err(error) => {
                    return Err(format!(
                        "Invalid workspace directory (symlinks are forbidden): {error}"
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Entry {
    pub fn stat(&self) -> Result<Option<Stat>, String> {
        match fs::statat(&self.parent, &self.name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(stat) => match FileType::from_raw_mode(stat.st_mode) {
                FileType::RegularFile | FileType::Directory => Ok(Some(stat)),
                _ => Err(format!(
                    "Symlinks and special files are forbidden: '{}'",
                    self.path.display()
                )),
            },
            Err(rustix::io::Errno::NOENT) => Ok(None),
            Err(error) => Err(format!("Cannot inspect '{}': {error}", self.path.display())),
        }
    }

    pub fn open_file(&self) -> Result<File, String> {
        let fd = fs::openat(
            &self.parent,
            &self.name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|e| format!("Cannot open file '{}': {e}", self.path.display()))?;
        let file = File::from(fd);
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err(format!(
                "Path '{}' must be a regular file",
                self.path.display()
            ));
        }
        Ok(file)
    }

    pub fn metadata(&self) -> Result<Metadata, String> {
        let stat = self
            .stat()?
            .ok_or_else(|| format!("File not found: '{}'", self.path.display()))?;
        if FileType::from_raw_mode(stat.st_mode) == FileType::Directory {
            let directory = open_directory(&self.parent, Path::new(&self.name))?;
            return File::from(directory).metadata().map_err(|e| e.to_string());
        }
        self.open_file()?.metadata().map_err(|e| e.to_string())
    }
}
