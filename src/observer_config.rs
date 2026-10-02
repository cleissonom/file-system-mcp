use std::io;
use std::path::{Component, Path, PathBuf};

pub fn database_path(cli: Option<PathBuf>, workspace: &Path) -> io::Result<PathBuf> {
    let path = cli
        .or_else(|| std::env::var_os("MCP_OBSERVER_DB").map(PathBuf::from))
        .unwrap_or(
            crate::runtime_paths::project_directory(&std::env::current_exe()?)
                .join(".data/observer.sqlite3"),
        );
    resolve_path(&path, &std::env::current_dir()?, workspace)
}

pub fn resolve_path(path: &Path, base: &Path, workspace: &Path) -> io::Result<PathBuf> {
    if path.as_os_str().to_string_lossy().trim().is_empty() {
        return Err(invalid(
            "MCP_OBSERVER_DB / --dashboard-db must name a SQLite file",
        ));
    }
    let path = crate::security::expand_tilde(path);
    let absolute = normalize(&if path.is_absolute() {
        path
    } else {
        base.join(path)
    });
    let name = absolute
        .file_name()
        .ok_or_else(|| invalid("Observer database must name a SQLite file"))?;
    let parent = canonical_parent(absolute.parent().expect("Absolute file has a parent"))?;
    let resolved = parent.join(name);
    if resolved.starts_with(workspace) {
        return Err(invalid(
            "Observer database must be outside the workspace; choose --dashboard-db or MCP_OBSERVER_DB",
        ));
    }
    Ok(resolved)
}

fn normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            value => normalized.push(value.as_os_str()),
        }
    }
    normalized
}

fn canonical_parent(parent: &Path) -> io::Result<PathBuf> {
    let mut ancestor = parent.to_path_buf();
    let mut missing = Vec::new();
    loop {
        match ancestor.canonicalize() {
            Ok(mut existing) => {
                for name in missing.iter().rev() {
                    existing.push(name);
                }
                return Ok(existing);
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                missing.push(
                    ancestor
                        .file_name()
                        .ok_or_else(|| invalid("Observer database parent is unavailable"))?
                        .to_owned(),
                );
                ancestor.pop();
            }
            Err(error) => return Err(error),
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
