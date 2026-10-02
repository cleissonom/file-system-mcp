use super::boundary::{Entry, duplicate};
use super::{MAX_COPY_BYTES, MAX_TEXT_BYTES, Workspace, content_hash};
use rustix::fd::OwnedFd;
use rustix::fs::{self, AtFlags, Mode, OFlags, RenameFlags};
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

pub(super) fn temporary_name() -> OsString {
    let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    format!(".mcp-tmp-{}-{sequence}", std::process::id()).into()
}

struct PendingFile {
    parent: OwnedFd,
    name: OsString,
    published: bool,
}

impl PendingFile {
    fn create(entry: &Entry) -> Result<(Self, File), String> {
        let parent = duplicate(&entry.parent)?;
        for _ in 0..100 {
            let name = temporary_name();
            match fs::openat(
                &parent,
                &name,
                OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::from_bits_truncate(0o600),
            ) {
                Ok(fd) => {
                    return Ok((
                        Self {
                            parent,
                            name,
                            published: false,
                        },
                        File::from(fd),
                    ));
                }
                Err(rustix::io::Errno::EXIST) => continue,
                Err(error) => return Err(format!("Cannot stage file: {error}")),
            }
        }
        Err("Cannot allocate a temporary workspace file".into())
    }

    fn publish(mut self, entry: &Entry, overwrite: bool) -> Result<(), String> {
        if overwrite {
            fs::renameat(&self.parent, &self.name, &entry.parent, &entry.name)
        } else {
            fs::renameat_with(
                &self.parent,
                &self.name,
                &entry.parent,
                &entry.name,
                RenameFlags::NOREPLACE,
            )
        }
        .map_err(|error| format!("Cannot publish '{}': {error}", entry.path.display()))?;
        self.published = true;
        Ok(())
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::unlinkat(&self.parent, &self.name, AtFlags::empty());
        }
    }
}

impl Workspace {
    pub fn write(
        &self,
        path: &Path,
        content: &[u8],
        overwrite: bool,
        expected_hash: Option<&str>,
    ) -> Result<String, String> {
        if content.len() > MAX_TEXT_BYTES {
            return Err("Content exceeds safety limit of 1 MB".into());
        }
        let entry = self.entry(path)?;
        self.check_write(&entry, overwrite, expected_hash)?;
        let mode = entry
            .stat()?
            .map(|stat| Mode::from_bits_truncate(stat.st_mode & 0o777));
        let (pending, mut file) = PendingFile::create(&entry)?;
        file.write_all(content).map_err(|e| e.to_string())?;
        sync_file(&file, mode)?;
        self.check_write(&entry, overwrite, expected_hash)?;
        pending.publish(&entry, overwrite)?;
        Ok(content_hash(content))
    }

    fn check_write(
        &self,
        entry: &Entry,
        overwrite: bool,
        expected_hash: Option<&str>,
    ) -> Result<(), String> {
        match entry.stat()? {
            Some(stat)
                if rustix::fs::FileType::from_raw_mode(stat.st_mode)
                    != rustix::fs::FileType::RegularFile =>
            {
                Err("Target must be a regular file, not a directory".into())
            }
            Some(_) if !overwrite => Err(format!(
                "File '{}' already exists. Set overwrite to true",
                entry.path.display()
            )),
            Some(_) => self.check_hash(entry, expected_hash),
            None if expected_hash.is_some() => {
                Err("Content hash conflict: expected file is missing".into())
            }
            None => Ok(()),
        }
    }

    pub(super) fn check_hash(&self, entry: &Entry, expected: Option<&str>) -> Result<(), String> {
        if let Some(expected) = expected {
            let mut content = Vec::new();
            entry
                .open_file()?
                .take(MAX_COPY_BYTES as u64 + 1)
                .read_to_end(&mut content)
                .map_err(|e| e.to_string())?;
            if content.len() > MAX_COPY_BYTES {
                return Err("File exceeds hash safety limit".into());
            }
            if content_hash(&content) != expected {
                return Err(format!(
                    "Content hash conflict: '{}' changed since it was read",
                    entry.path.display()
                ));
            }
        }
        Ok(())
    }

    pub(super) fn copy_file(
        &self,
        source: &Path,
        destination: &Entry,
        overwrite: bool,
        limit: usize,
    ) -> Result<usize, String> {
        let file = self.open_file(source)?;
        let mode = file.metadata().map_err(|e| e.to_string())?.permissions();
        self.check_write(destination, overwrite, None)?;
        let (pending, mut output) = PendingFile::create(destination)?;
        let copied = std::io::copy(&mut file.take(limit as u64 + 1), &mut output)
            .map_err(|e| e.to_string())?;
        if copied > limit as u64 {
            return Err("Copy exceeds safety limit".into());
        }
        use std::os::unix::fs::PermissionsExt;
        sync_file(
            &output,
            Some(Mode::from_bits_truncate((mode.mode() & 0o777) as _)),
        )?;
        self.check_write(destination, overwrite, None)?;
        pending.publish(destination, overwrite)?;
        Ok(copied as usize)
    }
}

fn sync_file(file: &File, mode: Option<Mode>) -> Result<(), String> {
    if let Some(mode) = mode {
        fs::fchmod(file, mode).map_err(|e| e.to_string())?;
    }
    file.sync_all()
        .map_err(|e| format!("Cannot sync staged file: {e}"))
}
