use super::*;

pub(super) fn validate_plan_extension(path: &Path) -> Result<(), SecurityError> {
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| SecurityError::InvalidPath("Filename cannot be empty".to_string()))?;

    let lower = file_name.to_lowercase();
    let is_markdown = lower.ends_with(".md") || lower.ends_with(".markdown");
    if !is_markdown {
        return Err(SecurityError::InvalidExtension(
            "Only markdown files (.md or .markdown) are allowed in plans/".to_string(),
        ));
    }

    if lower == ".md" || lower == ".markdown" {
        return Err(SecurityError::InvalidPath(
            "Filename must include a name before the extension".to_string(),
        ));
    }

    Ok(())
}

pub(super) fn validate_raw_filename(raw: &str) -> Result<String, SecurityError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(SecurityError::InvalidPath(
            "Filename cannot be empty".to_string(),
        ));
    }
    if trimmed.contains('\0') {
        return Err(SecurityError::InvalidPath(
            "Filename contains null byte".to_string(),
        ));
    }
    let normalized = trimmed.replace('\\', "/");
    if normalized.ends_with('/') {
        return Err(SecurityError::InvalidPath(
            "Target must be a file, not a directory".to_string(),
        ));
    }
    let input = Path::new(&normalized);
    for comp in input.components() {
        if comp == Component::ParentDir {
            return Err(SecurityError::DirectoryTraversal);
        }
    }
    Ok(normalized)
}

pub(super) fn validate_patch_extension(path: &Path) -> Result<(), SecurityError> {
    let file_name = path
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or_else(|| SecurityError::InvalidPath("Filename cannot be empty".to_string()))?;

    let lower = file_name.to_lowercase();
    let is_patch = lower.ends_with(".patch") || lower.ends_with(".diff");
    if !is_patch {
        return Err(SecurityError::InvalidExtension(
            "Only patch files (.patch or .diff) are allowed in patches/".to_string(),
        ));
    }

    if lower == ".patch" || lower == ".diff" {
        return Err(SecurityError::InvalidPath(
            "Filename must include a name before the extension".to_string(),
        ));
    }

    Ok(())
}

pub(super) fn extract_scoped_subpath(
    root: &Path,
    scope_dir: &Path,
    normalized: &str,
) -> Result<PathBuf, SecurityError> {
    let input = Path::new(normalized);
    let resolved_scope = if scope_dir.is_relative() {
        root.join(scope_dir)
    } else {
        scope_dir.to_path_buf()
    };
    let scope_leaf = scope_dir
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("scope");

    let mut clean = normalized;
    while let Some(rest) = clean.strip_prefix("./") {
        clean = rest;
    }

    if clean == scope_leaf || clean == format!("/{}", scope_leaf) {
        return Err(SecurityError::InvalidPath(format!(
            "Target must be a file, not the {} directory",
            scope_leaf
        )));
    }

    let rel_scope_str = resolved_scope
        .strip_prefix(root)
        .ok()
        .and_then(|p| p.to_str())
        .map(|s| s.replace('\\', "/"));

    if let Some(ref rel) = rel_scope_str
        && (clean == rel.as_str() || clean == format!("/{}", rel))
    {
        return Err(SecurityError::InvalidPath(format!(
            "Target must be a file, not the {} directory",
            scope_leaf
        )));
    }

    // Exact prefix of resolved_scope (e.g. /workspace/docs/plans/foo.md)
    if input.is_absolute() && input.starts_with(&resolved_scope) {
        return Ok(input.strip_prefix(&resolved_scope).unwrap().to_path_buf());
    }

    // Prefixed by root (e.g. /workspace/docs/plans/foo.md or /workspace/foo.md)
    if input.is_absolute()
        && let Ok(rel) = input.strip_prefix(root)
    {
        if let Ok(sub) = rel.strip_prefix(scope_dir) {
            return Ok(sub.to_path_buf());
        }
        if let Some(ref rel_scope) = rel_scope_str
            && let Ok(sub) = rel.strip_prefix(Path::new(rel_scope))
        {
            return Ok(sub.to_path_buf());
        }
        if let Ok(sub) = rel.strip_prefix(scope_leaf) {
            return Ok(sub.to_path_buf());
        }
        return Err(SecurityError::PathOutsideRoot);
    }

    // Prefixed by relative scope path (e.g. "docs/plans/foo.md" or "/docs/plans/foo.md")
    if let Some(ref rel) = rel_scope_str {
        let slash_prefix = format!("/{}/", rel);
        let plain_prefix = format!("{}/", rel);
        if let Some(rest) = clean.strip_prefix(&slash_prefix) {
            return Ok(PathBuf::from(rest));
        }
        if let Some(rest) = clean.strip_prefix(&plain_prefix) {
            return Ok(PathBuf::from(rest));
        }
    }

    // Prefixed by scope leaf name (e.g. "my_plans/foo.md" or "/my_plans/foo.md")
    let leaf_slash_prefix = format!("/{}/", scope_leaf);
    let leaf_plain_prefix = format!("{}/", scope_leaf);
    if let Some(rest) = clean.strip_prefix(&leaf_slash_prefix) {
        return Ok(PathBuf::from(rest));
    }
    if let Some(rest) = clean.strip_prefix(&leaf_plain_prefix) {
        return Ok(PathBuf::from(rest));
    }

    // Default scope prefixes "plans/" / "/plans/" or "patches/" / "/patches/"
    if scope_leaf != "plans"
        && let Some(rest) = clean
            .strip_prefix("/plans/")
            .or_else(|| clean.strip_prefix("plans/"))
    {
        return Ok(PathBuf::from(rest));
    }
    if scope_leaf != "patches"
        && let Some(rest) = clean
            .strip_prefix("/patches/")
            .or_else(|| clean.strip_prefix("patches/"))
    {
        return Ok(PathBuf::from(rest));
    }

    if input.is_absolute() {
        return Err(SecurityError::PathOutsideRoot);
    }

    Ok(PathBuf::from(clean))
}

pub(super) fn sanitize_scoped_subpath(
    root: &Path,
    scope_dir: &Path,
    raw: &str,
) -> Result<PathBuf, SecurityError> {
    let sub = extract_scoped_subpath(root, scope_dir, &validate_raw_filename(raw)?)?;
    let trimmed = sub.to_string_lossy().trim().trim_matches('/').to_string();
    if trimmed.is_empty() {
        return Err(SecurityError::InvalidPath(
            "Filename cannot be empty".to_string(),
        ));
    }
    Ok(PathBuf::from(trimmed))
}
