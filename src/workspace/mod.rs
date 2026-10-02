use rustix::fd::OwnedFd;
use sha2::{Digest, Sha256};
use std::fs::{File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};

mod aliases;
mod atomic;
mod boundary;
mod policy;
mod tree;

#[cfg(test)]
mod tests;

pub const MAX_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_COPY_BYTES: usize = 100 * 1024 * 1024;
pub const MAX_TREE_ENTRIES: usize = 10_000;
pub const MAX_TREE_DEPTH: usize = 64;

#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
    fd: OwnedFd,
}

#[derive(Debug)]
pub struct WalkEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

pub struct WalkResult {
    pub entries: Vec<WalkEntry>,
    pub truncated: bool,
}

pub fn content_hash(content: &[u8]) -> String {
    format!("{:x}", Sha256::digest(content))
}

impl Workspace {
    pub fn open(root: &Path) -> Result<Self, String> {
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        let fd = boundary::open_directory(rustix::fs::CWD, &root)?;
        Ok(Self { root, fd })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn relative(&self, raw: &str) -> Result<PathBuf, String> {
        boundary::relative(raw)
    }

    pub fn legacy_relative(&self, raw: &str) -> Result<PathBuf, String> {
        let trimmed = raw.trim();
        let path = Path::new(trimmed);
        if path.is_absolute() {
            let relative = path
                .strip_prefix(&self.root)
                .unwrap_or_else(|_| Path::new(trimmed.trim_start_matches('/')));
            return boundary::validate_relative(relative);
        }
        self.relative(trimmed)
    }

    pub fn open_file(&self, path: &Path) -> Result<File, String> {
        self.entry(path)?.open_file()
    }

    pub fn metadata(&self, path: &Path) -> Result<Metadata, String> {
        if path.as_os_str().is_empty() {
            return File::from(boundary::duplicate(&self.fd)?)
                .metadata()
                .map_err(|e| e.to_string());
        }
        self.entry(path)?.metadata()
    }

    pub fn exists(&self, path: &Path) -> Result<bool, String> {
        Ok(self.entry(path)?.stat()?.is_some())
    }

    pub fn read(&self, path: &Path, limit: usize) -> Result<Vec<u8>, String> {
        let file = self.open_file(path)?;
        let mut content = Vec::new();
        file.take(limit as u64 + 1)
            .read_to_end(&mut content)
            .map_err(|e| e.to_string())?;
        if content.len() > limit {
            return Err(format!("File exceeds safety limit of {limit} bytes"));
        }
        Ok(content)
    }

    pub fn scope(&self, configured: &Path) -> Result<PathBuf, String> {
        let path = if configured.is_absolute() {
            configured
                .strip_prefix(&self.root)
                .map_err(|_| "Directory is outside the workspace root")?
        } else {
            configured
        };
        let path = boundary::validate_relative(path)?;
        self.check_existing_ancestors(&path)?;
        Ok(path)
    }
}
