use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub fn derive_name(path: &Path, home: &Path) -> String {
    if path == home {
        return "home".to_string();
    }
    let relative_components: Vec<String> = if let Ok(rel) = path.strip_prefix(home) {
        rel.components()
            .map(|c| normalize_component(&c.as_os_str().to_string_lossy()))
            .collect()
    } else {
        // outside HOME -> "root" + absolute path components
        std::iter::once("root".to_string())
            .chain(
                path.components()
                    .filter(|c| c.as_os_str() != "/")
                    .map(|c| normalize_component(&c.as_os_str().to_string_lossy())),
            )
            .collect()
    };
    relative_components.join("-")
}

fn normalize_component(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() || c == '/' { '-' } else { c })
        .collect()
}

pub fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(p) = cur {
        if p.join(".claude-sandbox.toml").exists() {
            return Ok(p.to_path_buf());
        }
        if p.join(".git").exists() {
            return Ok(p.to_path_buf());
        }
        cur = p.parent();
    }
    Err(Error::ProjectNotFound(start.to_path_buf()))
}

pub fn short_hash(path: &std::path::Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    format!("{:08x}", (h.finish() & 0xFFFF_FFFF) as u32)
}
