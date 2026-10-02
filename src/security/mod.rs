use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub enum SecurityError {
    DirectoryTraversal,
    PathOutsideRoot,
    InvalidExtension(String),
    InvalidPath(String),
}

impl fmt::Display for SecurityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityError::DirectoryTraversal => {
                write!(f, "Directory traversal detected (use of '..')")
            }
            SecurityError::PathOutsideRoot => write!(f, "Path is outside the workspace root"),
            SecurityError::InvalidExtension(msg) => write!(f, "Invalid file extension: {}", msg),
            SecurityError::InvalidPath(msg) => write!(f, "Invalid path: {}", msg),
        }
    }
}

impl std::error::Error for SecurityError {}

mod config;
mod policy;
mod scoped;
mod scoped_helpers;

pub use config::{expand_tilde, resolve_scoped_dir};
pub use policy::is_denylisted;
use scoped_helpers::*;

#[cfg(test)]
mod tests;

pub(crate) use scoped::normalize_scoped_filename;
