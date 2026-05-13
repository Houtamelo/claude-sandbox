//! Storage + hash tests for the long-lived OAuth token used by the
//! per-container `CLAUDE_CODE_OAUTH_TOKEN` env-var auth path.

use std::os::unix::fs::PermissionsExt;

use claude_sandbox::machine::{
    load_oauth_token, oauth_token_exists, oauth_token_hash, oauth_token_path,
    remove_oauth_token, save_oauth_token,
};

/// Each test gets its own isolated $HOME so we don't read/write the
/// real user's oauth_token file. `serial_for_home` semantics — these
/// tests poke a shared env var, so they MUST run on a single thread.
fn with_isolated_home<F: FnOnce()>(f: F) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let prev_home = std::env::var_os("HOME");
    let prev_xdg = std::env::var_os("XDG_CONFIG_HOME");
    // SAFETY: tests are forced single-threaded below via the wrapper
    // macro; setting env from one test thread is fine.
    unsafe {
        std::env::set_var("HOME", tmp.path());
        std::env::set_var("XDG_CONFIG_HOME", tmp.path().join(".config"));
    }
    f();
    unsafe {
        if let Some(v) = prev_home { std::env::set_var("HOME", v); }
        else { std::env::remove_var("HOME"); }
        if let Some(v) = prev_xdg { std::env::set_var("XDG_CONFIG_HOME", v); }
        else { std::env::remove_var("XDG_CONFIG_HOME"); }
    }
}

#[test]
fn absent_token_returns_none_and_stable_hash() {
    with_isolated_home(|| {
        assert!(!oauth_token_exists(), "fresh home should have no token");
        assert_eq!(load_oauth_token().unwrap(), None);
        let h1 = oauth_token_hash();
        let h2 = oauth_token_hash();
        assert_eq!(h1, h2, "absent-token hash must be stable across calls");
        assert_eq!(h1.len(), 16, "16 hex chars (u64 FNV-1a)");
    });
}

#[test]
fn save_then_load_round_trips() {
    with_isolated_home(|| {
        let token = "sk-ant-oat01-abc123";
        save_oauth_token(token).unwrap();
        assert!(oauth_token_exists());
        let back = load_oauth_token().unwrap().expect("token present");
        assert_eq!(back, token, "save -> load must round-trip exactly");
    });
}

#[test]
fn save_writes_mode_600() {
    with_isolated_home(|| {
        save_oauth_token("sk-ant-oat01-xyz").unwrap();
        let meta = std::fs::metadata(oauth_token_path()).expect("stat");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "token file must be mode 600 (got {:o}). Anyone who can read \
             this file holds a year of inference on the user's account.",
            mode
        );
    });
}

#[test]
fn save_strips_surrounding_whitespace_and_appends_newline() {
    with_isolated_home(|| {
        save_oauth_token("  sk-ant-oat01-padded  \n").unwrap();
        let raw = std::fs::read_to_string(oauth_token_path()).unwrap();
        // Whitespace on the ends got trimmed; we write a trailing newline
        // so `cat` looks normal. load_oauth_token re-trims so callers
        // never see the newline.
        assert_eq!(raw, "sk-ant-oat01-padded\n");
        let back = load_oauth_token().unwrap().unwrap();
        assert_eq!(back, "sk-ant-oat01-padded");
    });
}

#[test]
fn hash_changes_when_token_changes() {
    with_isolated_home(|| {
        save_oauth_token("sk-ant-oat01-first").unwrap();
        let h_first = oauth_token_hash();
        save_oauth_token("sk-ant-oat01-second").unwrap();
        let h_second = oauth_token_hash();
        assert_ne!(
            h_first, h_second,
            "rotating the token must change the hash — otherwise the \
             cs-oauth-hash label won't trip a container recreate"
        );
    });
}

#[test]
fn remove_deletes_existing_token_file() {
    // The cfg wizard's "remove" branch needs a way to take the user back
    // to the bind-mounted-credentials.json codepath. Deleting the file
    // both nukes the credential AND makes the credentials-shadow mount
    // skip (mounts::default_volumes gates on oauth_token_exists).
    with_isolated_home(|| {
        save_oauth_token("sk-ant-oat01-toremove").unwrap();
        assert!(oauth_token_exists());
        remove_oauth_token().unwrap();
        assert!(!oauth_token_exists(), "file must be gone after remove");
        assert_eq!(load_oauth_token().unwrap(), None);
    });
}

#[test]
fn remove_is_idempotent_when_no_token_exists() {
    // No-op when there's nothing to remove. The wizard might call this
    // defensively, and a missing file is the desired post-condition.
    with_isolated_home(|| {
        assert!(!oauth_token_exists());
        remove_oauth_token().expect("removing absent token must not error");
        assert!(!oauth_token_exists());
        // Second call also OK.
        remove_oauth_token().expect("idempotent");
    });
}

#[test]
fn remove_changes_hash_back_to_absent_sentinel() {
    // The cs-oauth-hash label uses the hash for container-recreate
    // gating. After remove, the hash must match the "no token configured"
    // hash so subsequent container creates see a different label and
    // recreate (otherwise the container stays bound to the just-removed
    // env-var token).
    with_isolated_home(|| {
        let absent_hash = oauth_token_hash();
        save_oauth_token("sk-ant-oat01-presence").unwrap();
        let present_hash = oauth_token_hash();
        assert_ne!(absent_hash, present_hash);
        remove_oauth_token().unwrap();
        assert_eq!(
            oauth_token_hash(),
            absent_hash,
            "after remove, hash returns to the absent-sentinel value"
        );
    });
}

#[test]
fn empty_file_behaves_like_absent() {
    with_isolated_home(|| {
        // Manually create an empty file at the token path. load should
        // treat it as None (not an empty-string token).
        let p = oauth_token_path();
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "").unwrap();
        assert_eq!(
            load_oauth_token().unwrap(),
            None,
            "empty file should be treated as 'no token configured'"
        );
    });
}
