use std::path::Path;

use crate::config::edit::create_minimal;
use crate::error::Result;
use crate::registry;

pub fn migrate(container_name: &str, new_path: &Path) -> Result<()> {
    let toml = new_path.join(".claude-sandbox.toml");
    if !toml.exists() {
        create_minimal(&toml, container_name)?;
    }
    registry::upsert(container_name, new_path)
}
