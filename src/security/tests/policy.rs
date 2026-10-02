use super::*;

#[test]
fn test_denylist_git() {
    assert!(is_denylisted(Path::new(".git")));
    assert!(is_denylisted(Path::new(".git/config")));
    assert!(is_denylisted(Path::new("subrepo/.git/HEAD")));
}

#[test]
fn test_denylist_env() {
    assert!(is_denylisted(Path::new(".env")));
    assert!(is_denylisted(Path::new(".env.local")));
    assert!(is_denylisted(Path::new(".env.production")));
    assert!(is_denylisted(Path::new(".envrc")));
    assert!(is_denylisted(Path::new("service-a/.env")));
    assert!(is_denylisted(Path::new("service-a/.env.staging")));
}

#[test]
fn test_denylist_keys_and_certs() {
    assert!(is_denylisted(Path::new("server.pem")));
    assert!(is_denylisted(Path::new("sub/cert.pem")));
    assert!(is_denylisted(Path::new("private.key")));
    assert!(is_denylisted(Path::new("sub/auth.key")));
    assert!(is_denylisted(Path::new(".ssh/id_rsa")));
    assert!(is_denylisted(Path::new("id_ed25519")));
    assert!(is_denylisted(Path::new(".aws/credentials")));
}

#[test]
fn test_denylist_bastion() {
    assert!(is_denylisted(Path::new(".helpers/bastion.sh")));
    assert!(is_denylisted(Path::new("some/path/.helpers/bastion.sh")));
    assert!(is_denylisted(Path::new("bastion.sh")));
}

#[test]
fn test_allowed_paths() {
    assert!(!is_denylisted(Path::new("README.md")));
    assert!(!is_denylisted(Path::new("src/main.rs")));
    assert!(!is_denylisted(Path::new("package.json")));
    assert!(!is_denylisted(Path::new(".gitignore")));
}
