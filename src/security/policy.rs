use super::*;

/// Check if a relative path matches any hard-denylisted patterns.
pub fn is_denylisted(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    let normalized = path_str.replace('\\', "/");

    // Specific file/path denylists
    if normalized.contains(".helpers/bastion.sh") || normalized.ends_with("bastion.sh") {
        return true;
    }

    for comp in path.components() {
        if let Component::Normal(os_str) = comp {
            let s = os_str.to_string_lossy().to_lowercase();

            // Sensitive directories and files
            if s == ".git" || s == ".ssh" || s == ".aws" {
                return true;
            }
            if s.starts_with(".env") {
                return true;
            }
            if s.ends_with(".pem")
                || s.ends_with(".key")
                || s.contains(".key.")
                || s.contains(".pem.")
            {
                return true;
            }
            if s == "id_rsa" || s == "id_ed25519" || s == "id_dsa" || s == "id_ecdsa" {
                return true;
            }
            if s == "bastion.sh" || s.starts_with("bastion.sh.") {
                return true;
            }
        }
    }

    false
}
