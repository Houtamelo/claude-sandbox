use std::collections::BTreeMap;

use crate::error::Result;
use crate::podman::runner::Podman;

pub struct HookEnv {
    pub project_name: String,
    pub project_path: std::path::PathBuf,
    pub worktree_name: Option<String>,
}

pub fn run(
    podman: &Podman,
    container: &str,
    commands: &[String],
    env: &HookEnv,
    abort_on_failure: bool,
) -> Result<()> {
    if commands.is_empty() {
        return Ok(());
    }
    let mut env_pairs: BTreeMap<&str, String> = BTreeMap::new();
    env_pairs.insert("CS_PROJECT_NAME", env.project_name.clone());
    env_pairs.insert(
        "CS_PROJECT_PATH",
        env.project_path.display().to_string(),
    );
    if let Some(w) = &env.worktree_name {
        env_pairs.insert("CS_WORKTREE_NAME", w.clone());
    }

    let script = commands.join(" && ");
    let mut args: Vec<String> = vec!["exec".into()];
    for (k, v) in &env_pairs {
        args.push("--env".into());
        args.push(format!("{}={}", k, v));
    }
    args.push(container.into());
    args.push("bash".into());
    args.push("-c".into());
    args.push(script);

    match podman.run(&args) {
        Ok(_) => Ok(()),
        Err(e) => {
            if abort_on_failure {
                Err(e)
            } else {
                eprintln!("[warn] hook failed (continuing): {e}");
                Ok(())
            }
        }
    }
}
