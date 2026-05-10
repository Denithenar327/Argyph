use camino::{Utf8Path, Utf8PathBuf};

/// Check that a symlink target resolves within the given root.
///
/// Non-symlink paths are always considered safe. Symlinks are resolved
/// with [`std::fs::canonicalize`] and rejected if they cannot be resolved
/// or if the target lies outside `root`.
pub fn is_symlink_safe(path: &Utf8Path, root: &Utf8Path) -> bool {
    if !path.as_std_path().is_symlink() {
        return true;
    }
    let resolved = match std::fs::canonicalize(path.as_std_path()) {
        Ok(p) => p,
        Err(_) => return false,
    };
    let resolved = match Utf8PathBuf::from_path_buf(resolved) {
        Ok(p) => p,
        Err(_) => return false,
    };
    resolved.starts_with(root)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;

    #[test]
    fn non_symlink_is_safe() {
        let root = Utf8PathBuf::from("/tmp");
        let file = root.join("test.txt");
        assert!(is_symlink_safe(&file, &root));
    }

    #[cfg(unix)]
    #[test]
    fn symlink_outside_root_is_rejected() {
        let dir = std::env::temp_dir();
        let root_dir = dir.join("argyph_sym_root");
        let outside_dir = dir.join("argyph_sym_outside");
        let _ = std::fs::remove_dir_all(&root_dir);
        let _ = std::fs::remove_dir_all(&outside_dir);
        std::fs::create_dir(&root_dir).unwrap();
        std::fs::create_dir(&outside_dir).unwrap();

        let target = outside_dir.join("target.txt");
        std::fs::write(&target, b"outside").unwrap();

        let link = root_dir.join("link.txt");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let root = Utf8PathBuf::from_path_buf(root_dir.clone()).unwrap();
        let link_path = Utf8PathBuf::from_path_buf(link.clone()).unwrap();
        assert!(!is_symlink_safe(&link_path, &root));

        std::fs::remove_dir_all(&root_dir).unwrap();
        std::fs::remove_dir_all(&outside_dir).unwrap();
    }
}
