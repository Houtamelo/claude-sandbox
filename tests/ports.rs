use std::net::TcpListener;

use claude_sandbox::network::{parse, resolve, PortRequest};

#[test]
fn parses_preferred() {
    let r = parse("5173:5173").unwrap();
    assert_eq!(r, PortRequest { host: Some(5173), container: 5173, strict: false });
}

#[test]
fn parses_strict() {
    let r = parse("!8080:8080").unwrap();
    assert_eq!(r, PortRequest { host: Some(8080), container: 8080, strict: true });
}

#[test]
fn parses_ephemeral() {
    let r = parse(":3000").unwrap();
    assert_eq!(r, PortRequest { host: None, container: 3000, strict: false });
}

#[test]
fn rejects_missing_colon() {
    assert!(parse("hello").is_err());
}

#[test]
fn shift_picks_next_free() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = listener.local_addr().unwrap().port();
    let r = PortRequest { host: Some(taken), container: 9999, strict: false };
    let mapped = resolve(&[r]).unwrap();
    assert_eq!(mapped[0].container, 9999);
    assert_ne!(mapped[0].host, taken);
}

#[test]
fn strict_errors_when_unavailable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = listener.local_addr().unwrap().port();
    let r = PortRequest { host: Some(taken), container: 9999, strict: true };
    assert!(resolve(&[r]).is_err());
}
