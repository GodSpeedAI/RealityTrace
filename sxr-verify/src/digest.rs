//! A content digest of a verification target: what exactly was verified.
//!
//! `sha256:` over the sorted lines `<relative path>\0<kind>\0<sha256 hex>\n`, one per regular file
//! (kind `f`, hash of its bytes) and symlink (kind `l`, hash of its target path; never followed), under
//! `root`, skipping every `.git` directory. Paths use `/` separators. Two trees with the same files,
//! contents and links give the same digest; any change to one of them changes it.

use sha2::{Digest, Sha256};
use std::path::Path;

pub fn tree_digest(root: &Path) -> std::io::Result<String> {
    let mut lines: Vec<String> = Vec::new();
    walk(root, root, &mut lines)?;
    lines.sort();
    let mut h = Sha256::new();
    for l in &lines {
        h.update(l.as_bytes());
    }
    Ok(format!("sha256:{}", hex::encode(h.finalize())))
}

fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let ft = entry.file_type()?;
        let rel = path
            .strip_prefix(root)
            .expect("walked path is under root")
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if ft.is_symlink() {
            let target = std::fs::read_link(&path)?;
            let hash = hex::encode(Sha256::digest(target.as_os_str().as_encoded_bytes()));
            out.push(format!("{rel}\0l\0{hash}\n"));
        } else if ft.is_dir() {
            if entry.file_name() != ".git" {
                walk(root, &path, out)?;
            }
        } else if ft.is_file() {
            let hash = hex::encode(Sha256::digest(std::fs::read(&path)?));
            out.push(format!("{rel}\0f\0{hash}\n"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn digest_is_stable_and_sensitive_to_content_paths_and_links() {
        let a = tempdir().unwrap();
        std::fs::create_dir_all(a.path().join("src")).unwrap();
        std::fs::write(a.path().join("src/lib.rs"), "one").unwrap();
        std::fs::write(a.path().join("README"), "r").unwrap();
        let d1 = tree_digest(a.path()).unwrap();
        assert!(d1.starts_with("sha256:"));
        assert_eq!(d1, tree_digest(a.path()).unwrap(), "stable");

        // .git is ignored.
        std::fs::create_dir_all(a.path().join(".git/objects")).unwrap();
        std::fs::write(a.path().join(".git/HEAD"), "ref").unwrap();
        assert_eq!(d1, tree_digest(a.path()).unwrap(), ".git ignored");

        // Content change.
        std::fs::write(a.path().join("src/lib.rs"), "two").unwrap();
        let d2 = tree_digest(a.path()).unwrap();
        assert_ne!(d1, d2);
        std::fs::write(a.path().join("src/lib.rs"), "one").unwrap();
        assert_eq!(d1, tree_digest(a.path()).unwrap());

        // Rename (same bytes, different path).
        std::fs::rename(a.path().join("README"), a.path().join("README.md")).unwrap();
        assert_ne!(d1, tree_digest(a.path()).unwrap());
        std::fs::rename(a.path().join("README.md"), a.path().join("README")).unwrap();

        // A symlink is recorded by its target, not followed.
        std::os::unix::fs::symlink("/etc/passwd", a.path().join("link")).unwrap();
        let d3 = tree_digest(a.path()).unwrap();
        assert_ne!(d1, d3);
        std::fs::remove_file(a.path().join("link")).unwrap();
        std::os::unix::fs::symlink("/etc/hosts", a.path().join("link")).unwrap();
        assert_ne!(d3, tree_digest(a.path()).unwrap());
    }

    #[test]
    fn an_empty_file_differs_from_no_file() {
        let a = tempdir().unwrap();
        let empty = tree_digest(a.path()).unwrap();
        std::fs::write(a.path().join("x"), "").unwrap();
        assert_ne!(empty, tree_digest(a.path()).unwrap());
    }
}
