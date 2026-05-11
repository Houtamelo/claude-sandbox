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
    let res = podman.run_inherit(&[
        "build".into(),
        "-t".into(),
        "claude-sandbox:0.1".into(),
        "--build-arg".into(),
        format!("HOSTHOME={host_home}"),
        "-f".into(),
        dockerfile.display().to_string(),
        config_dir.display().to_string(),
    ]);
    let _ = std::fs::remove_file(&bin_dst);
    res
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
