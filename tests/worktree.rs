use std::path::{Path, PathBuf};

use claude_sandbox::worktree::commands::{
    build_worktree_add_args, parse_porcelain, BranchAction,
};

#[test]
fn add_args_create_named_after_worktree_uses_dash_b_with_worktree_name() {
    // Default path when the picker's `Branch` input is empty: create
    // a fresh branch named after the worktree itself. git form:
    // `worktree add -b <worktree-name> <dir>`.
    let args = build_worktree_add_args(
        "feat-x",
        Path::new("/work/.worktrees/feat-x"),
        &BranchAction::CreateNamedAfterWorktree,
    );
    assert_eq!(
        args,
        vec![
            "worktree".to_string(),
            "add".into(),
            "-b".into(),
            "feat-x".into(),
            "/work/.worktrees/feat-x".into(),
        ]
    );
}

#[test]
fn add_args_use_existing_branch_omits_dash_b() {
    // User typed an existing branch name. git checks out the existing
    // ref into the worktree dir. No `-b`. git form:
    // `worktree add <dir> <existing-branch>`.
    let args = build_worktree_add_args(
        "feat-x",
        Path::new("/work/.worktrees/feat-x"),
        &BranchAction::UseExisting("main".into()),
    );
    assert_eq!(
        args,
        vec![
            "worktree".to_string(),
            "add".into(),
            "/work/.worktrees/feat-x".into(),
            "main".into(),
        ]
    );
}

#[test]
fn add_args_create_named_uses_dash_b_with_supplied_name() {
    // User typed a branch name that doesn't yet exist AND confirmed
    // "create new branch with this name". git form:
    // `worktree add -b <new-branch> <dir>`. Worktree DIR is still
    // named after the worktree label, but the branch gets the
    // user-supplied name (which is the difference from
    // CreateNamedAfterWorktree — the names can diverge).
    let args = build_worktree_add_args(
        "feat-x",
        Path::new("/work/.worktrees/feat-x"),
        &BranchAction::CreateNamed("totally-different-branch-name".into()),
    );
    assert_eq!(
        args,
        vec![
            "worktree".to_string(),
            "add".into(),
            "-b".into(),
            "totally-different-branch-name".into(),
            "/work/.worktrees/feat-x".into(),
        ]
    );
}

#[test]
fn add_args_handle_paths_with_spaces() {
    // Sanity: paths get passed through as-is. git's CLI handles them
    // via positional args, not shell quoting.
    let args = build_worktree_add_args(
        "with space",
        Path::new("/work/.worktrees/with space"),
        &BranchAction::CreateNamedAfterWorktree,
    );
    assert_eq!(args.last().map(String::as_str), Some("/work/.worktrees/with space"));
}

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
