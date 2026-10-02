use super::*;
use std::fs;
use std::path::PathBuf;

fn temp_workspace() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().to_path_buf();
    (dir, path)
}

mod config;
mod patch_validate;
mod patch_write;
mod plan_write;
mod reads;
