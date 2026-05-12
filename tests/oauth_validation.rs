//! Unit + (optionally) integration tests for the OAuth token validator.
//!
//! Unit tests cover the pure `parse_validation` helper. The live test
//! actually shells to `curl` and talks to Anthropic; gate it on
//! `CLAUDE_SANDBOX_NET=1` so CI / offline runs don't break.

use claude_sandbox::machine::{
    parse_validation, validate_oauth_token, TokenValidation,
};

#[test]
fn http_401_means_invalid() {
    let r = parse_validation(true, "401");
    assert!(matches!(r, TokenValidation::Invalid { .. }), "got {r:?}");
}

#[test]
fn http_403_means_invalid() {
    let r = parse_validation(true, "403");
    assert!(matches!(r, TokenValidation::Invalid { .. }), "got {r:?}");
}

#[test]
fn http_400_means_valid_auth() {
    // 400 = bad request body. Auth was accepted, body was bad. From our
    // perspective (only auth matters) this is success.
    let r = parse_validation(true, "400");
    assert_eq!(r, TokenValidation::Valid);
}

#[test]
fn http_200_means_valid() {
    assert_eq!(parse_validation(true, "200"), TokenValidation::Valid);
}

#[test]
fn http_500_means_unknown() {
    // Anthropic outage — don't punish the user; let them through with a
    // warning higher up.
    let r = parse_validation(true, "500");
    assert!(matches!(r, TokenValidation::Unknown { .. }), "got {r:?}");
}

#[test]
fn http_000_means_unknown() {
    let r = parse_validation(false, "000");
    assert!(matches!(r, TokenValidation::Unknown { .. }), "got {r:?}");
}

#[test]
fn curl_failure_with_zero_code_is_unknown() {
    let r = parse_validation(false, "000");
    let TokenValidation::Unknown { reason } = r else {
        panic!("expected Unknown");
    };
    assert!(reason.contains("network") || reason.contains("timeout"));
}

/// Hits the real Anthropic API with an obviously-bad token. Gated:
/// `CLAUDE_SANDBOX_NET=1` to opt in (skips otherwise so offline test
/// runs don't fail).
#[test]
fn live_bad_token_returns_invalid() {
    if std::env::var("CLAUDE_SANDBOX_NET").ok().as_deref() != Some("1") {
        eprintln!("[skip] set CLAUDE_SANDBOX_NET=1 to run live API test");
        return;
    }
    let r = validate_oauth_token("sk-ant-oat01-obviously-not-real-token");
    assert!(
        matches!(r, TokenValidation::Invalid { .. }),
        "expected Invalid for a bogus token against the live API; got {r:?}"
    );
}
