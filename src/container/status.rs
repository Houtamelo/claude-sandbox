use crate::error::Result;
use crate::podman::runner::Podman;

pub struct Status {
    pub exists: bool,
    pub running: bool,
}

pub fn collect(podman: &Podman, name: &str) -> Result<Status> {
    let exists = podman.container_exists(name)?;
    let running = if exists { podman.container_running(name)? } else { false };
    Ok(Status { exists, running })
}

pub fn print(status: &Status, name: &str) {
    let state = match (status.exists, status.running) {
        (false, _) => "absent",
        (true, false) => "stopped",
        (true, true) => "running",
    };
    println!("container: {} ({})", name, state);
}
