use std::path::{Path, PathBuf};

pub fn project_directory(executable: &Path) -> PathBuf {
    let directory = executable
        .parent()
        .expect("Current executable has a parent");
    directory
        .ancestors()
        .find(|path| path.join("Cargo.toml").is_file())
        .unwrap_or(directory)
        .to_path_buf()
}
