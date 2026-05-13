//! `machine::binary_content_hash` produces a stable identifier for the
//! running binary so the container-recreate gate can detect that a fresh
//! `cargo install` (or any binary swap) needs to re-create the container
//! — even when machine.toml / project toml / oauth token haven't changed.
//!
//! Surfaced by the GPU keep-groups regression: adding new podman-create
//! args in source, reinstalling the binary, but the gate didn't fire
//! because every existing config-hash matched its stored label. The
//! container stayed pinned to the previous binary's args.

use claude_sandbox::machine::{binary_content_hash, binary_content_hash_of};

#[test]
fn hash_is_16_hex_chars_fnv1a_64() {
    // Mirror the toml/oauth-hash format so the label layout stays
    // consistent. 16 hex chars = 64-bit FNV-1a in hex.
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"any byte sequence").unwrap();
    let h = binary_content_hash_of(tmp.path()).unwrap();
    assert_eq!(h.len(), 16, "expected 16 hex chars; got {h:?}");
    assert!(h.chars().all(|c| c.is_ascii_hexdigit()), "non-hex char: {h:?}");
}

#[test]
fn hash_is_deterministic_for_same_bytes() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"hello claude-sandbox").unwrap();
    let h1 = binary_content_hash_of(tmp.path()).unwrap();
    let h2 = binary_content_hash_of(tmp.path()).unwrap();
    assert_eq!(h1, h2, "hash must be deterministic — otherwise the label gate would always trip");
}

#[test]
fn hash_changes_when_bytes_change() {
    // Critical: changing the binary content (the whole point of a
    // binary-hash gate) must shift the hash so the container-recreate
    // gate fires. Without this, the gate is a no-op.
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"version-A bytes").unwrap();
    let h1 = binary_content_hash_of(tmp.path()).unwrap();
    std::fs::write(tmp.path(), b"version-B bytes").unwrap();
    let h2 = binary_content_hash_of(tmp.path()).unwrap();
    assert_ne!(h1, h2, "same path, different bytes — hash must differ");
}

#[test]
fn empty_file_still_hashes_cleanly() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), b"").unwrap();
    let h = binary_content_hash_of(tmp.path()).unwrap();
    assert_eq!(h.len(), 16);
    // FNV-1a 64-bit offset basis with no input = 0xcbf29ce484222325
    assert_eq!(h, "cbf29ce484222325");
}

#[test]
fn convenience_helper_hashes_current_exe() {
    // The no-arg `binary_content_hash` reads std::env::current_exe.
    // Two calls return the same value (the running test binary doesn't
    // change underneath us during the test).
    let h1 = binary_content_hash();
    let h2 = binary_content_hash();
    assert_eq!(h1, h2);
    assert_eq!(h1.len(), 16);
    assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn missing_path_returns_sentinel_not_panic() {
    // The gate calls this on every claude-sandbox start. A bogus
    // current_exe (rare but possible: deleted binary, exotic FS)
    // shouldn't bring down the binary — surface a stable sentinel
    // so the container's label can record "unknown binary" rather
    // than crash.
    let h = binary_content_hash_of(std::path::Path::new("/nonexistent/path/to/claude-sandbox"));
    assert!(h.is_err(), "missing file should be an Err so the caller picks the sentinel explicitly");
}
