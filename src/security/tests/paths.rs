use super::*;

fn normalize(scope: &str, filename: &str, is_patch: bool) -> Result<PathBuf, SecurityError> {
    normalize_scoped_filename(
        Path::new("/workspace"),
        Path::new(scope),
        filename,
        is_patch,
    )
}

#[test]
fn plan_filename_aliases_preserve_the_subpath() {
    for filename in [
        "sub/detail.md",
        "plans/sub/detail.md",
        "/plans/sub/detail.md",
        "./plans/sub/detail.md",
        "/workspace/plans/sub/detail.md",
    ] {
        assert_eq!(
            normalize("plans", filename, false).unwrap(),
            PathBuf::from("sub/detail.md")
        );
    }
    assert_eq!(
        normalize("plans", "detail.markdown", false).unwrap(),
        PathBuf::from("detail.markdown")
    );
}

#[test]
fn patch_filename_aliases_preserve_the_subpath() {
    for filename in [
        "sub/change.patch",
        "patches/sub/change.patch",
        "/patches/sub/change.patch",
        "./patches/sub/change.patch",
        "/workspace/patches/sub/change.patch",
    ] {
        assert_eq!(
            normalize("patches", filename, true).unwrap(),
            PathBuf::from("sub/change.patch")
        );
    }
    assert_eq!(
        normalize("patches", "change.diff", true).unwrap(),
        PathBuf::from("change.diff")
    );
}

#[test]
fn custom_and_nested_scope_aliases_preserve_the_subpath() {
    for (scope, leaf, filename, is_patch) in [
        ("docs/plans", "plans", "detail.md", false),
        ("custom/diffs", "diffs", "change.diff", true),
        (
            "custom_architecture_plans",
            "custom_architecture_plans",
            "detail.md",
            false,
        ),
    ] {
        for prefix in [
            "".to_string(),
            format!("{scope}/"),
            format!("/{scope}/"),
            format!("{leaf}/"),
            format!("/{leaf}/"),
            format!("/workspace/{scope}/"),
        ] {
            let result = normalize(scope, &format!("{prefix}{filename}"), is_patch).unwrap();
            assert_eq!(result, PathBuf::from(filename));
        }
    }
}

#[test]
fn filename_normalization_rejects_traversal_and_external_absolute_paths() {
    for (scope, filename, is_patch) in [
        ("plans", "escape.md", false),
        ("patches", "escape.patch", true),
    ] {
        for prefix in ["../", "sub/../../", "..\\", "plans/../../"] {
            assert_eq!(
                normalize(scope, &format!("{prefix}{filename}"), is_patch).unwrap_err(),
                SecurityError::DirectoryTraversal
            );
        }
        for prefix in ["/etc/", "/workspace/outside/"] {
            assert_eq!(
                normalize(scope, &format!("{prefix}{filename}"), is_patch).unwrap_err(),
                SecurityError::PathOutsideRoot
            );
        }
    }
}

#[test]
fn filename_normalization_requires_the_scope_extension() {
    for filename in ["script.py", "config.json", "change.patch"] {
        assert!(matches!(
            normalize("plans", filename, false),
            Err(SecurityError::InvalidExtension(_))
        ));
    }
    for filename in ["script.py", "config.json", "detail.md"] {
        assert!(matches!(
            normalize("patches", filename, true),
            Err(SecurityError::InvalidExtension(_))
        ));
    }
    assert_eq!(
        normalize("plans", "UPPER.MD", false).unwrap(),
        PathBuf::from("UPPER.MD")
    );
    assert_eq!(
        normalize("patches", "UPPER.DIFF", true).unwrap(),
        PathBuf::from("UPPER.DIFF")
    );
}

#[test]
fn filename_normalization_requires_a_named_file() {
    for filename in [
        "",
        " ",
        ".md",
        ".markdown",
        "plans",
        "/plans",
        "dir.md/",
        "nul\0.md",
    ] {
        assert!(
            matches!(
                normalize("plans", filename, false),
                Err(SecurityError::InvalidPath(_))
            ),
            "{filename:?}"
        );
    }
    for filename in [".patch", ".diff", "patches", "/patches", "dir.patch/"] {
        assert!(
            matches!(
                normalize("patches", filename, true),
                Err(SecurityError::InvalidPath(_))
            ),
            "{filename:?}"
        );
    }
}
