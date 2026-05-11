use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub host_pid: i32,
    pub started_at: u64,
    pub container_exec_id: Option<String>,
}

pub fn claim_path(worktree_dir: &Path) -> PathBuf {
    worktree_dir.join(".cs-session")
}

pub fn write(worktree_dir: &Path) -> Result<Claim> {
    let claim = Claim {
        host_pid: std::process::id() as i32,
        started_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        container_exec_id: None,
    };
    let body = serde_json::to_string_pretty(&claim).map_err(|e| Error::Other(e.to_string()))?;
    std::fs::write(claim_path(worktree_dir), body)?;
    Ok(claim)
}

pub fn read(worktree_dir: &Path) -> Result<Option<Claim>> {
    let p = claim_path(worktree_dir);
    if !p.exists() {
        return Ok(None);
    }
    let body = std::fs::read_to_string(&p)?;
    let claim: Claim = serde_json::from_str(&body).map_err(|e| Error::Other(e.to_string()))?;
    Ok(Some(claim))
}

pub fn clear(worktree_dir: &Path) -> Result<()> {
    let p = claim_path(worktree_dir);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

pub fn pid_alive(pid: i32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid), None).is_ok()
}

pub enum ClaimState {
    Available,
    Active(Claim),
    Stale(Claim),
}

pub fn evaluate(worktree_dir: &Path) -> Result<ClaimState> {
    Ok(match read(worktree_dir)? {
        None => ClaimState::Available,
        Some(c) if pid_alive(c.host_pid) => ClaimState::Active(c),
        Some(c) => ClaimState::Stale(c),
    })
}
