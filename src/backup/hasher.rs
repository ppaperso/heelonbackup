//! SHA-256 checksums

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufReader};
use std::path::Path;

const BUFFER_SIZE: usize = 256 * 1024;

/// SHA-256 of a file, streamed; returns the hex digest and the number of bytes read
pub fn hash_file(path: &Path) -> io::Result<(String, u64)> {
    let mut reader = BufReader::with_capacity(BUFFER_SIZE, File::open(path)?);
    let mut hasher = Sha256::new();
    let size = io::copy(&mut reader, &mut hasher)?;
    Ok((hex::encode(hasher.finalize()), size))
}

/// SHA-256 of an in-memory buffer
pub fn hash_bytes(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const HELLO_WORLD: &str = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";

    #[test]
    fn hashes_bytes() {
        assert_eq!(hash_bytes(b"hello world"), HELLO_WORLD);
    }

    #[test]
    fn hashes_files() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(b"hello world").unwrap();
        let (hash, size) = hash_file(file.path()).unwrap();
        assert_eq!(hash, HELLO_WORLD);
        assert_eq!(size, 11);
    }

    #[test]
    fn missing_file_is_an_error() {
        assert!(hash_file(Path::new("/nonexistent/heelonbackup")).is_err());
    }
}
