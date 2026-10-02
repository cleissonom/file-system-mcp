use super::*;

/// Normalize a legacy scoped filename without reading or creating any filesystem paths.
pub(crate) fn normalize_scoped_filename(
    root: &Path,
    scope_dir: &Path,
    filename: &str,
    is_patch: bool,
) -> Result<PathBuf, SecurityError> {
    let subpath = sanitize_scoped_subpath(root, scope_dir, filename)?;
    if is_patch {
        validate_patch_extension(&subpath)?;
    } else {
        validate_plan_extension(&subpath)?;
    }
    Ok(subpath)
}
