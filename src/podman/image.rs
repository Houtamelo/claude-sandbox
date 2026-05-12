use crate::error::Result;
use crate::paths;
use crate::podman::runner::Podman;
use crate::registry;

pub fn rebuild(podman: &Podman) -> Result<()> {
    // Build context lives in the cache dir, not in ~/.config — keeps the
    // user-override slot (`~/.config/claude-sandbox/`) free of transient
    // files. Re-created each rebuild to guarantee no stale binary/Dockerfile.
    let build_dir = paths::cache_dir().join("build");
    if build_dir.exists() {
        std::fs::remove_dir_all(&build_dir)?;
    }
    std::fs::create_dir_all(&build_dir)?;

    // Resolve the Dockerfile through the three-tier lookup
    // (user override -> /usr/share/claude-sandbox/ -> embedded). Write
    // the resolved contents into the build context so podman sees them
    // regardless of which tier provided them.
    let dockerfile_asset = crate::assets::resolve_dockerfile()?;
    let dockerfile = build_dir.join("Dockerfile");
    std::fs::write(&dockerfile, &dockerfile_asset.contents)?;

    // When the resolved Dockerfile comes from the user-override slot,
    // surface what's actually being used. Two cases:
    //   - identical to the embedded default → leftover from an older
    //     `make install`; we can safely fall back to embedded. Hint
    //     the user toward `claude-sandbox cfg` for cleanup.
    //   - differs from embedded → real override (manual edit OR stale
    //     copy from an older shipped version). Either way the user
    //     needs to decide; refusing to use it would surprise users who
    //     intentionally edited.
    if let crate::assets::AssetSource::UserOverride(p) = &dockerfile_asset.source {
        match crate::assets::dockerfile_override_state() {
            crate::assets::OverrideState::MatchesEmbedded => {
                eprintln!(
                    "note: ~/.config Dockerfile at {} is identical to the embedded \
                     default — safe to remove. Run `claude-sandbox cfg` to clean up.",
                    p.display()
                );
            }
            crate::assets::OverrideState::DiffersFromEmbedded => {
                eprintln!(
                    "note: using user-override Dockerfile at {} (differs from this \
                     binary's embedded default). If this is stale auto-deployed \
                     cruft from an older claude-sandbox, run `claude-sandbox cfg` to \
                     refresh or delete it.",
                    p.display()
                );
            }
            crate::assets::OverrideState::Absent => {
                // Can't happen: source was UserOverride so the file existed
                // at resolve time. Race window is harmless.
            }
        }
    }

    // Copy our own binary alongside the Dockerfile so `COPY claude-sandbox ...` succeeds.
    let bin_src = std::env::current_exe()?;
    let bin_dst = build_dir.join("claude-sandbox");
    std::fs::copy(&bin_src, &bin_dst)?;
    // Sandbox-self-awareness CLAUDE.md baked into the image at /CLAUDE.md.
    // We embed it in the binary so rebuilds always pick up the latest version,
    // and write it to the build context alongside the binary so the COPY in
    // the Dockerfile can pick it up.
    let claude_md_dst = build_dir.join("CLAUDE.md");
    std::fs::write(&claude_md_dst, include_str!("../../assets/CLAUDE.md"))?;
    // Pass the host's HOME so the image's `claude` user has a matching home
    // path. Claude Code's setup-state cache is keyed by HOME — if the
    // in-container HOME doesn't match, claude inside starts fresh every time.
    let host_home = paths::home().display().to_string();
    // Detect host claude version so the in-container claude matches exactly.
    // Newer/older claude versions have settings.json schema drift and emit
    // different notifications (e.g. 2.1.138 vs 2.1.126 differ on skill-listing
    // truncation behavior). Falls back to "stable" if no host claude — but
    // log the reason so users don't silently get a mismatched image.
    let host_claude_version = match detect_host_claude_version() {
        Ok(v) => v,
        Err(e) => {
            eprintln!(
                "warning: couldn't detect host `claude` version ({e}); \
                 falling back to \"stable\". The in-container claude may \
                 not match the host's. Add `claude` to PATH or upgrade it, \
                 then `claude-sandbox rebuild` again."
            );
            "stable".into()
        }
    };
    // Host UID + base image + machine.toml hash come from
    // `claude-sandbox cfg`. We require setup at this layer too —
    // `rebuild` is rejected by the gate in main.rs before we get here,
    // but be defensive in case someone calls the library directly.
    let machine_cfg = crate::machine::require_setup_done()?;
    let host_uid = machine_cfg.host.uid;
    let base_image = machine_cfg.image.base.clone();
    let extra_packages = machine_cfg.image.extra_packages.join(" ");
    let machine_hash = crate::machine::content_hash(&machine_cfg);
    let res = podman.run_inherit(&[
        "build".into(),
        "-t".into(),
        "claude-sandbox:0.1".into(),
        "--build-arg".into(),
        format!("BASE_IMAGE={base_image}"),
        "--build-arg".into(),
        format!("HOSTHOME={host_home}"),
        "--build-arg".into(),
        format!("CLAUDE_VERSION={host_claude_version}"),
        "--build-arg".into(),
        format!("HOSTUID={host_uid}"),
        "--build-arg".into(),
        format!("CS_MACHINE_HASH={machine_hash}"),
        "--build-arg".into(),
        format!("EXTRA_PACKAGES={extra_packages}"),
        "-f".into(),
        dockerfile.display().to_string(),
        build_dir.display().to_string(),
    ]);
    // build_dir is in the cache; leave it for inspection on failure and
    // we'll wipe + recreate on the next rebuild call.
    res
}

/// Read the `cs-machine-hash` label off the locally-tagged image.
/// Returns `None` if the image doesn't exist locally or has no such
/// label (e.g. legacy image from before the label was added).
pub fn image_machine_hash(podman: &Podman) -> Option<String> {
    let v = podman
        .run_json(&[
            "image".into(),
            "inspect".into(),
            "--format".into(),
            "{{json .Config.Labels}}".into(),
            "claude-sandbox:0.1".into(),
        ])
        .ok()?;
    v.get("cs-machine-hash")
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

/// Typed failure mode for [`detect_host_claude_version`]. The earlier
/// `.ok()?` design collapsed every failure to `None`, which then
/// silently fell back to `"stable"` — the in-container claude ended
/// up at whatever stable happened to be that day, never matching the
/// host, and the user got no signal. The caller now logs each variant
/// so the staleness is visible at rebuild time.
#[derive(Debug, PartialEq, Eq)]
pub enum ClaudeDetectError {
    /// `Command::new("claude").output()` failed at the OS level — most
    /// commonly because `claude` isn't on the subprocess `PATH`. The
    /// host's interactive shell might still find it; subprocess env
    /// differs.
    NotFound(String),
    /// `claude --version` ran but exited non-zero. Captures stderr so
    /// the user can see what claude itself reported (e.g. a corrupt
    /// install or an auth-state error).
    ExitNonZero { code: Option<i32>, stderr: String },
    /// `claude --version` ran cleanly but its output didn't contain a
    /// recognizable version token. Future-proofing — guards against an
    /// upstream banner-line change silently breaking the pin.
    UnparsableOutput { stdout: String },
}

impl std::fmt::Display for ClaudeDetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(reason) => {
                write!(f, "`claude` not found on PATH: {reason}")
            }
            Self::ExitNonZero { code, stderr } => {
                let code = code.map(|c| c.to_string()).unwrap_or_else(|| "<signal>".into());
                let s = stderr.trim();
                if s.is_empty() {
                    write!(f, "`claude --version` exited with code {code}")
                } else {
                    write!(f, "`claude --version` exited with code {code}: {s}")
                }
            }
            Self::UnparsableOutput { stdout } => {
                write!(
                    f,
                    "`claude --version` printed unparsable output: {:?}",
                    stdout.trim()
                )
            }
        }
    }
}

impl std::error::Error for ClaudeDetectError {}

/// Parse `claude --version` output (e.g. "2.1.139 (Claude Code)") into
/// the version string. Pure function — `Command` invocation lives in
/// [`detect_host_claude_version`]; tests target this directly.
///
/// Requires the first whitespace-separated token to contain at least
/// one digit so we don't misidentify a banner like "Claude 2.1.139"
/// (the banner-first form would silently make `claude` the version).
pub fn parse_claude_version_stdout(
    stdout: &str,
) -> std::result::Result<String, ClaudeDetectError> {
    stdout
        .split_whitespace()
        .next()
        .filter(|t| t.chars().any(|c| c.is_ascii_digit()))
        .map(|s| s.to_string())
        .ok_or_else(|| ClaudeDetectError::UnparsableOutput {
            stdout: stdout.to_string(),
        })
}

pub fn detect_host_claude_version() -> std::result::Result<String, ClaudeDetectError> {
    let out = std::process::Command::new("claude")
        .arg("--version")
        .output()
        .map_err(|e| ClaudeDetectError::NotFound(e.to_string()))?;
    if !out.status.success() {
        return Err(ClaudeDetectError::ExitNonZero {
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        });
    }
    parse_claude_version_stdout(&String::from_utf8_lossy(&out.stdout))
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
