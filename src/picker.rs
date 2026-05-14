use std::path::Path;

use dialoguer::{Confirm, Select};

use crate::error::{Error, Result};
use crate::worktree::claim::{evaluate, ClaimState};
use crate::worktree::commands::{branch_exists, list as list_worktrees, BranchAction};
use crate::worktree::WorktreeInfo;

pub enum Choice {
    Main,
    Existing(String),
    /// Create a new worktree at `.worktrees/<name>`. The
    /// [`BranchAction`] is resolved interactively in [`pick`] from
    /// the user's branch input + a confirm prompt when the typed
    /// branch doesn't exist locally.
    New(String, BranchAction),
    Quit,
}

pub fn pick(project: &Path) -> Result<Choice> {
    let entries = build_entries(project)?;
    let labels: Vec<String> = entries.iter().map(label).collect();
    let mut labels_with_actions = labels.clone();
    labels_with_actions.push("+ new worktree".into());
    labels_with_actions.push("quit".into());

    let idx = Select::new()
        .with_prompt("Choose")
        .items(&labels_with_actions)
        .default(0)
        .interact()
        .map_err(|e| Error::Other(format!("picker: {e}")))?;

    if idx == labels_with_actions.len() - 1 {
        return Ok(Choice::Quit);
    }
    if idx == labels_with_actions.len() - 2 {
        let name: String = dialoguer::Input::new()
            .with_prompt("Worktree name")
            .interact_text()
            .map_err(|e| Error::Other(format!("input: {e}")))?;
        let branch: String = dialoguer::Input::new()
            .with_prompt(format!(
                "Branch (empty = create new branch named '{name}')"
            ))
            .allow_empty(true)
            .interact_text()
            .map_err(|e| Error::Other(format!("input: {e}")))?;
        let action = if branch.is_empty() {
            BranchAction::CreateNamedAfterWorktree
        } else if branch_exists(project, &branch) {
            BranchAction::UseExisting(branch)
        } else {
            // Typed branch doesn't exist locally. Don't silently
            // either-error-out or silently-create — ask explicitly.
            let create = Confirm::new()
                .with_prompt(format!(
                    "Branch named '{branch}' does not exist, create new branch?"
                ))
                .default(true)
                .interact()
                .map_err(|e| Error::Other(format!("confirm: {e}")))?;
            if create {
                BranchAction::CreateNamed(branch)
            } else {
                return Ok(Choice::Quit);
            }
        };
        return Ok(Choice::New(name, action));
    }

    let entry = &entries[idx];
    Ok(if entry.name == "main" {
        Choice::Main
    } else {
        Choice::Existing(entry.name.clone())
    })
}

fn build_entries(project: &Path) -> Result<Vec<WorktreeInfo>> {
    list_worktrees(project)
}

fn label(w: &WorktreeInfo) -> String {
    let state = if w.name == "main" {
        "main".to_string()
    } else {
        match evaluate(&w.path).unwrap_or(ClaimState::Available) {
            ClaimState::Available => "available".into(),
            ClaimState::Active(c) => format!(
                "in-use: host PID {} since epoch {}",
                c.host_pid, c.started_at
            ),
            ClaimState::Stale(c) => format!("stale claim PID {} — will reclaim", c.host_pid),
        }
    };
    format!("{}  [{}]", w.name, state)
}

pub fn has_worktrees(project: &Path) -> bool {
    let p = project.join(".worktrees");
    p.is_dir() && std::fs::read_dir(&p).map(|r| r.count() > 0).unwrap_or(false)
}
