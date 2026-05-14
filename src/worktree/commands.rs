use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

use super::WorktreeInfo;

pub fn list(project: &Path) -> Result<Vec<WorktreeInfo>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["worktree", "list", "--porcelain"])
        .output()?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(parse_porcelain(
        std::str::from_utf8(&out.stdout).unwrap_or(""),
        project,
    ))
}

pub fn parse_porcelain(text: &str, project: &Path) -> Vec<WorktreeInfo> {
    let mut out: Vec<WorktreeInfo> = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(p) = path.take() {
                out.push(WorktreeInfo {
                    name: classify(&p, project),
                    path: p,
                    branch: branch.take(),
                });
            }
            path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = Some(rest.trim_start_matches("refs/heads/").to_string());
        }
    }
    if let Some(p) = path.take() {
        out.push(WorktreeInfo {
            name: classify(&p, project),
            path: p,
            branch,
        });
    }
    out
}

fn classify(path: &Path, project: &Path) -> String {
    if path == project {
        "main".to_string()
    } else if let Ok(rel) = path.strip_prefix(project.join(".worktrees")) {
        rel.to_string_lossy().to_string()
    } else {
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "?".to_string())
    }
}

/// What to do with the branch when creating a worktree. Three cases
/// the picker resolves to:
///
/// - `CreateNamedAfterWorktree`: user left the branch input empty.
///   Create a fresh branch named after the worktree label itself.
///   `git worktree add -b <worktree-name> <dir>`.
/// - `UseExisting(name)`: user typed a branch name that already exists
///   in the repo. Check that ref out into the worktree dir.
///   `git worktree add <dir> <name>`.
/// - `CreateNamed(name)`: user typed a branch name that DOESN'T exist
///   yet AND confirmed "create new". Create with that name.
///   `git worktree add -b <name> <dir>`. Different from
///   `CreateNamedAfterWorktree` because branch name and worktree dir
///   can diverge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BranchAction {
    CreateNamedAfterWorktree,
    UseExisting(String),
    CreateNamed(String),
}

/// Pure: build the args for `git <args>` (without the `git -C <project>`
/// prefix, which the caller supplies). Extracted so unit tests can
/// assert the argument shape without spawning git.
pub fn build_worktree_add_args(
    worktree_name: &str,
    dir: &Path,
    action: &BranchAction,
) -> Vec<String> {
    let dir_str = dir.display().to_string();
    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];
    match action {
        BranchAction::CreateNamedAfterWorktree => {
            args.push("-b".into());
            args.push(worktree_name.to_string());
            args.push(dir_str);
        }
        BranchAction::UseExisting(b) => {
            args.push(dir_str);
            args.push(b.clone());
        }
        BranchAction::CreateNamed(b) => {
            args.push("-b".into());
            args.push(b.clone());
            args.push(dir_str);
        }
    }
    args
}

/// Returns true if `git -C <project> rev-parse --verify --quiet refs/heads/<branch>`
/// succeeds. Used by the picker to decide whether to prompt "create
/// new branch?" before delegating to [`add`].
pub fn branch_exists(project: &Path, branch: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["rev-parse", "--verify", "--quiet"])
        .arg(format!("refs/heads/{branch}"))
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn add(project: &Path, name: &str, action: BranchAction) -> Result<PathBuf> {
    let dir = project.join(".worktrees").join(name);
    if dir.exists() {
        return Err(Error::Other(format!("worktree {} already exists", name)));
    }
    std::fs::create_dir_all(project.join(".worktrees"))?;
    let args = build_worktree_add_args(name, &dir, &action);
    let status = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(&args)
        .status()?;
    if !status.success() {
        return Err(Error::Other(format!("git worktree add failed for {name}")));
    }
    ensure_gitignore_entry(project)?;
    Ok(dir)
}

fn ensure_gitignore_entry(project: &Path) -> Result<()> {
    let p = project.join(".gitignore");
    let needle = ".worktrees/\n";
    let current = std::fs::read_to_string(&p).unwrap_or_default();
    if !current.contains(".worktrees/") {
        let mut s = current;
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(needle);
        std::fs::write(&p, s)?;
    }
    Ok(())
}

pub fn remove(project: &Path, name: &str) -> Result<()> {
    let dir = project.join(".worktrees").join(name);
    let status = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["worktree", "remove", "--force"])
        .arg(&dir)
        .status()?;
    if !status.success() {
        return Err(Error::Other(format!("git worktree remove failed for {name}")));
    }
    // git worktree prune is automatic post-remove but explicit doesn't hurt.
    let _ = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["worktree", "prune"])
        .status();
    Ok(())
}

pub fn current(cwd: &Path, project: &Path) -> String {
    if cwd == project {
        return "main".to_string();
    }
    if let Ok(rel) = cwd.strip_prefix(project.join(".worktrees")) {
        if let Some(first) = rel.components().next() {
            return first.as_os_str().to_string_lossy().to_string();
        }
    }
    "main".to_string()
}
