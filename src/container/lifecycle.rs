use crate::error::Result;
use crate::podman::args::{rm_args, start_args, stop_args};
use crate::podman::runner::Podman;

pub fn ensure_running(podman: &Podman, name: &str) -> Result<()> {
    if !podman.container_running(name)? {
        crate::step!("Starting container");
        podman.run(&start_args(name))?;
    }
    Ok(())
}

pub fn stop(podman: &Podman, name: &str, on_stop: &[String], project: &std::path::Path) -> Result<()> {
    if !on_stop.is_empty() && podman.container_running(name)? {
        crate::step!("Running on_stop hooks ({} step(s))", on_stop.len());
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
    crate::step!("Stopping container");
    podman.run(&stop_args(name))?;
    Ok(())
}

pub fn down(podman: &Podman, name: &str) -> Result<()> {
    crate::step!("Destroying container and its named home volume");
    podman.run(&rm_args(name))?;
    let _ = crate::registry::remove(name);
    let vol = format!("cs-{}-home", name);
    let _ = podman.run(&["volume".into(), "rm".into(), "--force".into(), vol]);
    Ok(())
}
