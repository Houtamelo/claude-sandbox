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

pub fn add(project: &Path, name: &str, branch: Option<&str>) -> Result<PathBuf> {
    let dir = project.join(".worktrees").join(name);
    if dir.exists() {
        return Err(Error::Other(format!("worktree {} already exists", name)));
    }
    std::fs::create_dir_all(project.join(".worktrees"))?;
    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];
    if let Some(b) = branch {
        args.push(dir.display().to_string());
        args.push(b.to_string());
    } else {
        args.push("-b".into());
        args.push(name.to_string());
        args.push(dir.display().to_string());
    }
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
