use super::*;

#[test]
fn test_expand_tilde() {
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = PathBuf::from(home);
        assert_eq!(expand_tilde(Path::new("~")), home_path);
        assert_eq!(
            expand_tilde(Path::new("~/test/path")),
            home_path.join("test/path")
        );
    }
    assert_eq!(
        expand_tilde(Path::new("/var/log")),
        PathBuf::from("/var/log")
    );
    assert_eq!(
        expand_tilde(Path::new("  relative/path  ")),
        PathBuf::from("relative/path")
    );
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = PathBuf::from(home);
        assert_eq!(
            expand_tilde(Path::new("  ~/test/path  ")),
            home_path.join("test/path")
        );
    }
}

#[test]
fn test_resolve_scoped_dir() {
    let root = Path::new("/workspace");

    // Configured relative path
    let res = resolve_scoped_dir(Some(Path::new("custom_plans")), &[], "plans", root);
    assert_eq!(res, root.join("custom_plans"));

    // Default fallback
    let res = resolve_scoped_dir(None, &[], "plans", root);
    assert_eq!(res, root.join("plans"));

    // Empty string or whitespace fallback
    let res = resolve_scoped_dir(Some(Path::new("")), &[], "plans", root);
    assert_eq!(res, root.join("plans"));
    let res = resolve_scoped_dir(Some(Path::new("   ")), &[], "plans", root);
    assert_eq!(res, root.join("plans"));

    // Absolute path
    let abs = Path::new("/var/plans");
    let res = resolve_scoped_dir(Some(abs), &[], "plans", root);
    assert_eq!(res, abs);
}
