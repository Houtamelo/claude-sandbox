use std::fs;

use tempfile::tempdir;

use claude_sandbox::config::edit::{create_minimal, set_name};

#[test]
fn creates_with_name_and_header() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join(".claude-sandbox.toml");
    create_minimal(&p, "documents-projects-spire").unwrap();
    let body = fs::read_to_string(&p).unwrap();
    assert!(body.starts_with("# claude-sandbox config"));
    assert!(body.contains("name = \"documents-projects-spire\""));
}

#[test]
fn create_minimal_is_idempotent() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join(".claude-sandbox.toml");
    create_minimal(&p, "a").unwrap();
    fs::write(&p, "# already custom\nname = \"a\"\n").unwrap();
    create_minimal(&p, "different").unwrap();
    let body = fs::read_to_string(&p).unwrap();
    assert!(body.contains("# already custom"));
    assert!(body.contains("\"a\""));
}

#[test]
fn rename_preserves_comments() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("c.toml");
    fs::write(&p, "# header\n\nname = \"old\" # inline\n").unwrap();
    set_name(&p, "new").unwrap();
    let body = fs::read_to_string(&p).unwrap();
    assert!(body.contains("# header"));
    assert!(body.contains("# inline"));
    assert!(body.contains("\"new\""));
}
