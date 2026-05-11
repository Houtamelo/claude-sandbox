use crate::error::Result;
use crate::paths;
use crate::podman::runner::Podman;
use crate::registry;

pub fn rebuild(podman: &Podman) -> Result<()> {
    let config_dir = paths::config_dir();
    let dockerfile = config_dir.join("Dockerfile");
    // Copy our own binary alongside the Dockerfile so `COPY claude-sandbox ...` succeeds.
    let bin_src = std::env::current_exe()?;
    let bin_dst = config_dir.join("claude-sandbox");
    std::fs::copy(&bin_src, &bin_dst)?;
    // Pass the host's HOME so the image's `claude` user has a matching home
    // path. Claude Code's setup-state cache is keyed by HOME — if the
    // in-container HOME doesn't match, claude inside starts fresh every time.
    let host_home = paths::home().display().to_string();
    // Detect host claude version so the in-container claude matches exactly.
    // Newer/older claude versions have settings.json schema drift and emit
    // different notifications (e.g. 2.1.138 vs 2.1.126 differ on skill-listing
    // truncation behavior). Falls back to "stable" if no host claude.
    let host_claude_version = detect_host_claude_version().unwrap_or_else(|| "stable".into());
    let res = podman.run_inherit(&[
        "build".into(),
        "-t".into(),
        "claude-sandbox:0.1".into(),
        "--build-arg".into(),
        format!("HOSTHOME={host_home}"),
        "--build-arg".into(),
        format!("CLAUDE_VERSION={host_claude_version}"),
        "-f".into(),
        dockerfile.display().to_string(),
        config_dir.display().to_string(),
    ]);
    let _ = std::fs::remove_file(&bin_dst);
    res
}

/// Parse `claude --version` output (e.g. "2.1.126 (Claude Code)") into the
/// version string. Returns None if claude isn't installed or output is unexpected.
fn detect_host_claude_version() -> Option<String> {
    let out = std::process::Command::new("claude")
        .arg("--version")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Take the first whitespace-separated token (the semver).
    stdout.split_whitespace().next().map(|s| s.to_string())
}

pub fn recreate_all(podman: &Podman) -> Result<()> {
    let reg = registry::load()?;
    for (name, path) in reg.entries.iter() {
        eprintln!("Recreate {name} at {}? [y/N]", path.display());
        let mut buf = String::new();
        std::io::stdin().read_line(&mut buf).ok();
        if buf.trim().eq_ignore_ascii_case("y") {
            let _ = podman.run(&crate::podman::args::rm_args(name));
            // Container will be recreated on next `claude-sandbox start` in that project.
        }
    }
    Ok(())
}
