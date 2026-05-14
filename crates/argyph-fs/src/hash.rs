use camino::{Utf8Path, Utf8PathBuf};
use rayon::prelude::*;
use std::io;

/// BLAKE3 hash output — 32 bytes (256 bits).
///
/// `Display` and `Debug` formats produce lowercase hexadecimal.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Blake3Hash([u8; 32]);

impl Blake3Hash {
    /// Return the raw 32-byte hash.
    #[inline]
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for Blake3Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Blake3Hash({self})")
    }
}

impl std::fmt::Display for Blake3Hash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl From<[u8; 32]> for Blake3Hash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8; 32]> for Blake3Hash {
    fn as_ref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl AsRef<[u8]> for Blake3Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Compute the BLAKE3 hash of a file's contents, parallelized via rayon.
///
/// Reads the entire file into memory. Callers should enforce a size cap
/// before calling this on untrusted file paths.
pub fn hash_file(path: &Utf8Path) -> io::Result<Blake3Hash> {
    let data = std::fs::read(path.as_std_path())?;
    let hash = blake3::hash(&data);
    Ok(Blake3Hash(*hash.as_bytes()))
}

/// Compute BLAKE3 hashes for multiple file-data pairs in parallel using rayon.
pub fn hash_files_parallel(files: &[(Utf8PathBuf, Vec<u8>)]) -> Vec<(Utf8PathBuf, Blake3Hash)> {
    files
        .par_iter()
        .map(|(path, data)| {
            let hash = blake3::hash(data);
            (path.clone(), Blake3Hash(*hash.as_bytes()))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use camino::Utf8PathBuf;
    use std::io::Write;

    fn temp_utf8_path(name: &str) -> Utf8PathBuf {
        let dir = std::env::temp_dir();
        Utf8PathBuf::from_path_buf(dir.join(name)).unwrap()
    }

    #[test]
    fn hash_deterministic() {
        let path = temp_utf8_path("argyph_test_hash_det");
        let mut f = std::fs::File::create(path.as_std_path()).unwrap();
        f.write_all(b"hello arghash").unwrap();
        drop(f);

        let h1 = hash_file(&path).unwrap();
        let h2 = hash_file(&path).unwrap();
        assert_eq!(h1, h2);

        std::fs::remove_file(path.as_std_path()).unwrap();
    }

    #[test]
    fn hash_different_content() {
        let p1 = temp_utf8_path("argyph_test_hash_a");
        let p2 = temp_utf8_path("argyph_test_hash_b");
        std::fs::write(p1.as_std_path(), b"aaa").unwrap();
        std::fs::write(p2.as_std_path(), b"bbb").unwrap();

        let h1 = hash_file(&p1).unwrap();
        let h2 = hash_file(&p2).unwrap();
        assert_ne!(h1, h2);

        std::fs::remove_file(p1.as_std_path()).unwrap();
        std::fs::remove_file(p2.as_std_path()).unwrap();
    }

    #[test]
    fn display_is_hex() {
        let hash = Blake3Hash::from([0u8; 32]);
        let s = hash.to_string();
        assert_eq!(s.len(), 64);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_missing_file() {
        let path = temp_utf8_path("argyph_nonexistent_xyz");
        let _ = std::fs::remove_file(path.as_std_path());
        assert!(hash_file(&path).is_err());
    }
}
