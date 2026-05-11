use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub entries: BTreeMap<String, PathBuf>,
}

fn registry_path() -> PathBuf {
    paths::data_dir().join("registry.json")
}

pub fn load() -> Result<Registry> {
    let p = registry_path();
    if !p.exists() {
        return Ok(Registry::default());
    }
    let body = std::fs::read_to_string(p)?;
    Ok(serde_json::from_str(&body).unwrap_or_default())
}

pub fn save(reg: &Registry) -> Result<()> {
    let p = registry_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(reg).unwrap_or_else(|_| "{}".into());
    std::fs::write(p, body)?;
    Ok(())
}

pub fn upsert(name: &str, path: &Path) -> Result<()> {
    let mut reg = load()?;
    reg.entries.insert(name.to_string(), path.to_path_buf());
    save(&reg)
}

pub fn remove(name: &str) -> Result<()> {
    let mut reg = load()?;
    reg.entries.remove(name);
    save(&reg)
}
