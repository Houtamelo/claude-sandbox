use std::path::Path;

use crate::config::edit::set_name;
use crate::error::{Error, Result};
use crate::podman::runner::Podman;

pub fn rename(
    podman: &Podman,
    project: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<()> {
    if podman.container_exists(new_name)? {
        return Err(Error::NameCollision(new_name.into(), project.to_path_buf()));
    }
    if podman.container_exists(old_name)? {
        podman.run(&["rename".into(), old_name.into(), new_name.into()])?;
    }
    let toml = project.join(".claude-sandbox.toml");
    if toml.exists() {
        set_name(&toml, new_name)?;
    }
    Ok(())
}
