use super::Workspace;
use super::boundary::{duplicate, validate_relative};
use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, FileType, Mode, OFlags, Stat};
use std::path::{Path, PathBuf};

impl Workspace {
    pub fn actual_path(&self, path: &Path) -> Result<PathBuf, String> {
        let relative = validate_relative(path)?;
        let mut remaining = relative.components();
        let mut fd = duplicate(&self.fd)?;
        while let Some(component) = remaining.next() {
            match open_existing(&fd, Path::new(component.as_os_str()))? {
                Some(next) => fd = next,
                None => {
                    let mut actual = self.handle_relative_path(&fd)?;
                    actual.push(component.as_os_str());
                    for suffix in remaining {
                        actual.push(suffix.as_os_str());
                    }
                    return Ok(actual);
                }
            }
        }
        self.handle_relative_path(&fd)
    }

    fn handle_relative_path(&self, fd: &OwnedFd) -> Result<PathBuf, String> {
        check_type(fs::fstat(fd).map_err(|error| error.to_string())?)?;
        let actual = handle_path(fd)?;
        let relative = actual
            .strip_prefix(&self.root)
            .map_err(|_| "Opened entry is no longer inside the workspace root")?;
        validate_relative(relative)
    }
}

fn open_existing(parent: &OwnedFd, name: &Path) -> Result<Option<OwnedFd>, String> {
    match fs::statat(parent, name, AtFlags::SYMLINK_NOFOLLOW) {
        Ok(stat) => check_type(stat)?,
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(format!("Cannot resolve workspace path: {error}")),
    }
    let fd = fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|error| format!("Cannot resolve workspace path without symlinks: {error}"))?;
    check_type(fs::fstat(&fd).map_err(|error| error.to_string())?)?;
    Ok(Some(fd))
}

fn check_type(stat: Stat) -> Result<(), String> {
    if stat.st_nlink == 0 {
        return Err("Workspace entry changed during path resolution".into());
    }
    match FileType::from_raw_mode(stat.st_mode) {
        FileType::RegularFile | FileType::Directory => Ok(()),
        _ => Err("Symlinks and special filesystem objects are forbidden".into()),
    }
}

#[cfg(target_os = "macos")]
fn handle_path(fd: &OwnedFd) -> Result<PathBuf, String> {
    use std::os::unix::ffi::OsStringExt;
    let path = fs::getpath(fd)
        .map_err(|error| format!("Cannot inspect workspace handle path: {error}"))?;
    Ok(std::ffi::OsString::from_vec(path.into_bytes()).into())
}

#[cfg(target_os = "linux")]
fn handle_path(fd: &OwnedFd) -> Result<PathBuf, String> {
    use std::os::fd::AsRawFd;
    // This trusted kernel link describes the live handle; no caller supplies its path.
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())).map_err(|error| {
        format!("Cannot inspect workspace handle path; Linux requires /proc/self/fd: {error}")
    })
}
