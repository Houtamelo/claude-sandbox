use std::path::Path;

use dialoguer::Select;

use crate::error::{Error, Result};
use crate::worktree::claim::{evaluate, ClaimState};
use crate::worktree::commands::list as list_worktrees;
use crate::worktree::WorktreeInfo;

pub enum Choice {
    Main,
    Existing(String),
    New(String, Option<String>),
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
            .with_prompt("Branch (empty = new branch from HEAD)")
            .allow_empty(true)
            .interact_text()
            .map_err(|e| Error::Other(format!("input: {e}")))?;
        let branch = if branch.is_empty() { None } else { Some(branch) };
        return Ok(Choice::New(name, branch));
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
