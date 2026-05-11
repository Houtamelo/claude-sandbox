use std::path::Path;

use claude_sandbox::terminal::title_sequence;

#[test]
fn formats_main_worktree_with_basename() {
    let s = title_sequence(Path::new("/home/user/Documents/projects/scone"), None);
    assert_eq!(s, "\x1b]0;Claude - scone - main\x07");
}

#[test]
fn formats_named_worktree() {
    let s = title_sequence(
        Path::new("/home/user/Documents/projects/scone"),
        Some("336-consolidate-sketch-reveal"),
    );
    assert_eq!(
        s,
        "\x1b]0;Claude - scone - 336-consolidate-sketch-reveal\x07"
    );
}

#[test]
fn falls_back_to_full_path_when_no_basename() {
    // `/` has no file_name. Should render the path as-is rather than panic
    // or produce "Claude -  - main".
    let s = title_sequence(Path::new("/"), None);
    assert_eq!(s, "\x1b]0;Claude - / - main\x07");
}

#[test]
fn starts_with_osc_zero_introducer_and_ends_with_bel() {
    // Guards against a refactor that silently switches to OSC 2 (window-
    // title-only) or to the ST terminator (\x1b\\), which some terminals
    // don't accept.
    let s = title_sequence(Path::new("/x/proj"), Some("w"));
    assert!(
        s.starts_with("\x1b]0;"),
        "title sequence must start with `ESC ] 0 ;` (OSC 0); got bytes {:?}",
        s.as_bytes()
    );
    assert!(
        s.ends_with('\x07'),
        "title sequence must end with BEL (\\x07); got {:?}",
        s.as_bytes()
    );
}
