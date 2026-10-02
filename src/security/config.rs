use super::*;

/// Expands leading `~` or `~/` in a path using HOME / USERPROFILE environment variables.
pub fn expand_tilde(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim();
    if trimmed == "~" {
        if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
            return PathBuf::from(home);
        }
    } else if let Some(rest) = trimmed
        .strip_prefix("~/")
        .or_else(|| trimmed.strip_prefix("~\\"))
        && let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(trimmed)
}

/// Resolves a scoped directory (like plans or patches) from CLI input, env vars, or default.
pub fn resolve_scoped_dir(
    configured: Option<&Path>,
    env_vars: &[&str],
    default_name: &str,
    canonical_root: &Path,
) -> PathBuf {
    let candidate = configured
        .filter(|p| !p.as_os_str().is_empty() && !p.to_string_lossy().trim().is_empty())
        .map(expand_tilde)
        .or_else(|| {
            env_vars.iter().find_map(|var| {
                std::env::var(var)
                    .ok()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| expand_tilde(Path::new(s.trim())))
            })
        });

    match candidate {
        Some(p) => {
            if p.is_relative() {
                canonical_root.join(p)
            } else {
                p
            }
        }
        None => canonical_root.join(default_name),
    }
}
