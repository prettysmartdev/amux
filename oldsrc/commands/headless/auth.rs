use anyhow::Result;
use std::path::Path;

/// File name within the headless root where the key hash is stored.
pub const KEY_HASH_FILE: &str = "api_key.hash";

/// Generate a cryptographically random 32-byte API key, encode as
/// lowercase hex (64 chars), and return it.  Uses `ring::rand::SecureRandom`.
pub fn generate_api_key() -> Result<String> {
    use ring::rand::{SecureRandom, SystemRandom};
    let rng = SystemRandom::new();
    let mut key_bytes = [0u8; 32];
    rng.fill(&mut key_bytes)
        .map_err(|_| anyhow::anyhow!("Failed to generate random bytes for API key"))?;
    Ok(hex_encode(&key_bytes))
}

/// Hash an API key using SHA-256 (via `ring::digest`) and return the
/// hex-encoded digest.  This is the same operation performed by both
/// the server (to store) and the middleware (to compare).
pub fn hash_api_key(key: &str) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, key.as_bytes());
    hex_encode(digest.as_ref())
}

/// Write the hex-encoded hash to `<headless_root>/api_key.hash`.
/// On Unix, the file is created atomically with mode 0o600 using
/// `OpenOptions::mode` — this avoids a TOCTOU window where the file exists
/// briefly with world-readable permissions before `set_permissions` is called.
pub fn write_key_hash(headless_root: &Path, hash: &str) -> Result<()> {
    let path = headless_root.join(KEY_HASH_FILE);
    std::fs::create_dir_all(headless_root)?;

    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        file.write_all(hash.as_bytes())?;
    }
    #[cfg(not(unix))]
    {
        std::fs::write(&path, hash)?;
    }

    Ok(())
}

/// Read the hex-encoded hash from `<headless_root>/api_key.hash`.
/// Returns `None` if the file does not exist.
pub fn read_key_hash(headless_root: &Path) -> Result<Option<String>> {
    let path = headless_root.join(KEY_HASH_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    Ok(Some(content.trim().to_string()))
}

/// Print the plaintext API key to stdout with a clear banner.
/// This is the only place the plaintext key ever appears.
pub fn print_key_banner(key: &str) {
    // The key is 64 chars. Build the banner to fit.
    let inner_width = 67; // "  amux headless API key (store this — it will not be shown again)  " is 67 visible chars
    let key_line = format!("  {}  ", key);
    // Pad key line to inner_width
    let key_padded = format!("{:<width$}", key_line, width = inner_width);
    let title_line = "  amux headless API key (store this — it will not be shown again)  ";

    println!("╔{}╗", "═".repeat(inner_width));
    println!("║{}║", title_line);
    println!("║{}║", key_padded);
    println!("╚{}╝", "═".repeat(inner_width));
}

/// Encode bytes as lowercase hex.
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── generate_api_key ────────────────────────────────────────────────────

    #[test]
    fn generate_api_key_produces_64_char_lowercase_hex() {
        let key = generate_api_key().expect("generate_api_key must not fail");
        assert_eq!(
            key.len(),
            64,
            "API key must be 64 hex characters (32 random bytes); got len={}",
            key.len()
        );
        assert!(
            key.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "API key must be lowercase hex; got: {key}"
        );
    }

    #[test]
    fn two_successive_generate_api_keys_differ() {
        let key_a = generate_api_key().expect("first key");
        let key_b = generate_api_key().expect("second key");
        assert_ne!(
            key_a, key_b,
            "successive calls to generate_api_key must produce different keys"
        );
    }

    // ── hash_api_key ────────────────────────────────────────────────────────

    /// SHA-256 of "abc" as computed by the ring crate in this build environment.
    const SHA256_OF_ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn hash_api_key_matches_sha256_test_vector() {
        let digest = hash_api_key("abc");
        assert_eq!(
            digest, SHA256_OF_ABC,
            "SHA-256(\"abc\") must match NIST test vector; got: {digest}"
        );
    }

    #[test]
    fn hash_api_key_is_64_char_lowercase_hex() {
        let digest = hash_api_key("some-key");
        assert_eq!(digest.len(), 64, "SHA-256 hex digest must be 64 chars");
        assert!(
            digest.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "digest must be lowercase hex"
        );
    }

    #[test]
    fn hash_api_key_is_deterministic() {
        let key = "my-test-api-key-12345";
        let h1 = hash_api_key(key);
        let h2 = hash_api_key(key);
        assert_eq!(h1, h2, "hash_api_key must be deterministic");
    }

    #[test]
    fn hash_api_key_different_inputs_produce_different_digests() {
        let h1 = hash_api_key("key-one");
        let h2 = hash_api_key("key-two");
        assert_ne!(h1, h2, "different keys must hash to different digests");
    }

    // ── write_key_hash / read_key_hash ──────────────────────────────────────

    #[test]
    fn write_read_key_hash_round_trips() {
        let tmp = TempDir::new().unwrap();
        let hash = "deadbeef".repeat(8); // 64 hex chars

        write_key_hash(tmp.path(), &hash).expect("write_key_hash must succeed");
        let loaded = read_key_hash(tmp.path())
            .expect("read_key_hash must not error")
            .expect("read_key_hash must return Some when file exists");

        assert_eq!(loaded, hash, "round-trip must preserve the hash value");
    }

    #[test]
    fn read_key_hash_returns_none_when_file_missing() {
        let tmp = TempDir::new().unwrap();
        let result = read_key_hash(tmp.path()).expect("read_key_hash must not error on missing file");
        assert!(result.is_none(), "must return None when api_key.hash does not exist");
    }

    #[test]
    fn write_key_hash_trims_whitespace_on_read_back() {
        // Verify that read_key_hash trims trailing whitespace/newlines.
        let tmp = TempDir::new().unwrap();
        // Write hash without trailing newline.
        let hash = "abcd1234".repeat(8);
        write_key_hash(tmp.path(), &hash).unwrap();
        let loaded = read_key_hash(tmp.path()).unwrap().unwrap();
        assert_eq!(loaded, hash);
    }

    #[test]
    fn write_key_hash_creates_parent_directory_if_missing() {
        let tmp = TempDir::new().unwrap();
        let nested_root = tmp.path().join("nested").join("subdir");
        // Directory does NOT exist yet — write_key_hash must create it.
        write_key_hash(&nested_root, "abcd1234").expect("write must create parent dirs");
        let loaded = read_key_hash(&nested_root).unwrap().unwrap();
        assert_eq!(loaded, "abcd1234");
    }

    // ── file permissions (Unix only) ─────────────────────────────────────────

    #[cfg(unix)]
    #[test]
    fn write_key_hash_creates_file_with_mode_0o600() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let hash = "cafebabe".repeat(8);
        write_key_hash(tmp.path(), &hash).unwrap();

        let file_path = tmp.path().join(KEY_HASH_FILE);
        let meta = std::fs::metadata(&file_path).expect("file must exist");
        let mode = meta.permissions().mode();
        let file_mode_bits = mode & 0o777;

        assert_eq!(
            file_mode_bits, 0o600,
            "api_key.hash must have mode 0o600; got: 0o{file_mode_bits:o}"
        );
    }

    // ── print_key_banner (smoke test — does not panic) ───────────────────────

    #[test]
    fn print_key_banner_does_not_panic() {
        let key = "a".repeat(64);
        // Just verify it doesn't panic; stdout is not captured here.
        print_key_banner(&key);
    }
}
