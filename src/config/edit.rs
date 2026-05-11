use std::path::Path;

use toml_edit::{value, DocumentMut};

use crate::error::{Error, Result};

const HEADER: &str = "# claude-sandbox config — see `claude-sandbox docs`\n";

pub fn create_minimal(path: &Path, name: &str) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let body = format!("{HEADER}\nname = \"{name}\"\n");
    std::fs::write(path, body)
        .map_err(|e| Error::Config(format!("writing {}: {e}", path.display())))?;
    Ok(())
}

pub fn set_name(path: &Path, new_name: &str) -> Result<()> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
    let mut doc: DocumentMut = raw
        .parse()
        .map_err(|e| Error::Config(format!("editing {}: {e}", path.display())))?;

    // Preserve inline comments (value-level decor suffix) when replacing the string.
    let suffix = doc["name"]
        .as_value()
        .and_then(|v| v.decor().suffix())
        .cloned();

    let mut new_item = value(new_name);
    if let (Some(s), Some(v)) = (suffix, new_item.as_value_mut()) {
        v.decor_mut().set_suffix(s);
    }
    doc["name"] = new_item;

    std::fs::write(path, doc.to_string())
        .map_err(|e| Error::Config(format!("writing {}: {e}", path.display())))?;
    Ok(())
}
