use crate::error::Result;
use crate::podman::args::{rm_args, start_args, stop_args};
use crate::podman::runner::Podman;

pub fn ensure_running(podman: &Podman, name: &str) -> Result<()> {
    if !podman.container_running(name)? {
        podman.run(&start_args(name))?;
    }
    Ok(())
}

pub fn stop(podman: &Podman, name: &str, on_stop: &[String], project: &std::path::Path) -> Result<()> {
    if !on_stop.is_empty() && podman.container_running(name)? {
        crate::hooks::run(
            podman,
            name,
            on_stop,
            &crate::hooks::HookEnv {
                project_name: name.to_string(),
                project_path: project.to_path_buf(),
                worktree_name: None,
            },
            false,
            crate::hooks::HookUser::Root,
        )?;
    }
    podman.run(&stop_args(name))?;
    Ok(())
}

pub fn down(podman: &Podman, name: &str) -> Result<()> {
    podman.run(&rm_args(name))?;
    let _ = crate::registry::remove(name);
    let vol = format!("cs-{}-home", name);
    let _ = podman.run(&["volume".into(), "rm".into(), "--force".into(), vol]);
    Ok(())
}
