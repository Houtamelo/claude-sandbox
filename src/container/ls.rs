use std::collections::HashSet;

use crate::error::Result;
use crate::podman::runner::Podman;
use crate::registry;

pub fn ls(podman: &Podman, orphans_only: bool, with_size: bool) -> Result<()> {
    // Find every container we manage. We tag them with `cs-managed=1` at
    // create time; the label survives rename. Container names have no
    // fixed prefix (they derive from project path), so name-pattern
    // filtering is unreliable — use the label.
    let out = podman.run_json(&[
        "ps".into(),
        "-a".into(),
        "--filter".into(),
        "label=cs-managed=1".into(),
        "--format".into(),
        "json".into(),
    ])?;
    let mut reg = registry::load()?;
    let arr = out.as_array().cloned().unwrap_or_default();
    let live: HashSet<String> = arr
        .iter()
        .filter_map(|c| {
            c.get("Names")
                .and_then(|n| n.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect();

    // Self-heal: drop registry entries whose container no longer exists.
    let stale: Vec<String> = reg
        .entries
        .keys()
        .filter(|name| !live.contains(name.as_str()))
        .cloned()
        .collect();
    if !stale.is_empty() {
        for name in &stale {
            reg.entries.remove(name);
        }
        registry::save(&reg)?;
    }

    for c in arr {
        let name = c
            .get("Names")
            .and_then(|n| n.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .unwrap_or("?");
        let state = c.get("State").and_then(|s| s.as_str()).unwrap_or("?");
        let path = reg.entries.get(name).cloned();
        let is_orphan = match &path {
            Some(p) => !p.exists(),
            None => true,
        };
        if orphans_only && !is_orphan {
            continue;
        }
        let path_disp = path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<no registry entry>".into());
        let size_disp = if with_size {
            let sz = c.get("Size").and_then(|s| s.as_str()).unwrap_or("?");
            format!("\t{sz}")
        } else {
            String::new()
        };
        let orphan_tag = if is_orphan { " [orphan]" } else { "" };
        println!("{name}\t{state}\t{path_disp}{size_disp}{orphan_tag}");
    }
    Ok(())
}
