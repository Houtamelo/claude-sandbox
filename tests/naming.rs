use std::path::PathBuf;

use claude_sandbox::project::derive_name;

#[test]
fn name_under_home_uses_relative_components() {
    let home = PathBuf::from("/home/user");
    let path = PathBuf::from("/home/user/Documents/projects/spire");
    assert_eq!(derive_name(&path, &home), "documents-projects-spire");
}

#[test]
fn name_outside_home_uses_root_prefix() {
    let home = PathBuf::from("/home/user");
    let path = PathBuf::from("/srv/repos/spire");
    assert_eq!(derive_name(&path, &home), "root-srv-repos-spire");
}

#[test]
fn whitespace_collapses_to_dash() {
    let home = PathBuf::from("/home/user");
    let path = PathBuf::from("/home/user/My Projects/Cool Tool");
    assert_eq!(derive_name(&path, &home), "my-projects-cool-tool");
}

#[test]
fn home_itself_is_just_home() {
    let home = PathBuf::from("/home/user");
    assert_eq!(derive_name(&home, &home), "home");
}

use claude_sandbox::project::short_hash;

#[test]
fn short_hash_is_stable_and_eight_hex_chars() {
    let p = PathBuf::from("/home/u/p");
    let a = short_hash(&p);
    let b = short_hash(&p);
    assert_eq!(a, b);
    assert_eq!(a.len(), 8);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
}
