use std::path::PathBuf;

use claude_sandbox::worktree::commands::parse_porcelain;

#[test]
fn parses_main_and_worktree() {
    let project = PathBuf::from("/work");
    let text = "worktree /work\nHEAD abcd\nbranch refs/heads/main\n\nworktree /work/.worktrees/feat-x\nHEAD efgh\nbranch refs/heads/feat-x\n";
    let v = parse_porcelain(text, &project);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].name, "main");
    assert_eq!(v[0].branch.as_deref(), Some("main"));
    assert_eq!(v[1].name, "feat-x");
    assert_eq!(v[1].path, PathBuf::from("/work/.worktrees/feat-x"));
    assert_eq!(v[1].branch.as_deref(), Some("feat-x"));
}
