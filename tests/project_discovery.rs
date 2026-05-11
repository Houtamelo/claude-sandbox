use std::fs;

use tempfile::tempdir;

use claude_sandbox::project::find_project_root;

#[test]
fn finds_dir_with_toml() {
    let tmp = tempdir().unwrap();
    let proj = tmp.path().join("p");
    let sub = proj.join("a/b/c");
    fs::create_dir_all(&sub).unwrap();
    fs::write(proj.join(".claude-sandbox.toml"), "name = \"p\"\n").unwrap();

    assert_eq!(find_project_root(&sub).unwrap(), proj);
}

#[test]
fn finds_dir_with_git() {
    let tmp = tempdir().unwrap();
    let proj = tmp.path().join("p");
    let sub = proj.join("a/b");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir(proj.join(".git")).unwrap();

    assert_eq!(find_project_root(&sub).unwrap(), proj);
}

#[test]
fn toml_wins_over_git_when_closer() {
    let tmp = tempdir().unwrap();
    let outer = tmp.path().join("outer");
    let inner = outer.join("inner");
    let cwd = inner.join("sub");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir(outer.join(".git")).unwrap();
    fs::write(inner.join(".claude-sandbox.toml"), "").unwrap();

    assert_eq!(find_project_root(&cwd).unwrap(), inner);
}

#[test]
fn errors_when_no_marker_anywhere() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("a/b");
    fs::create_dir_all(&sub).unwrap();

    assert!(find_project_root(&sub).is_err());
}
