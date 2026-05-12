use std::process::ExitCode;

use clap::Parser;

use claude_sandbox::cli::{CsCli, Cmd, HostCli};
use claude_sandbox::container::{exec::exec_into, lifecycle, status as st};
use claude_sandbox::config::{edit as cfg_edit, load_global_merged, ConfigFile};
use claude_sandbox::error::Result;
use claude_sandbox::paths;
use claude_sandbox::podman::runner::Podman;
use claude_sandbox::project::{derive_name, find_project_root};
use claude_sandbox::logging;

const DEFAULT_IMAGE: &str = "claude-sandbox:0.1";

fn load_cfg(project: &std::path::Path) -> Result<ConfigFile> {
    let toml_path = project.join(".claude-sandbox.toml");
    load_global_merged(if toml_path.exists() { Some(&toml_path) } else { None })
}

/// Resolve the effective `claude` flags for this launch. Per-project
/// `claude_flags` (if set) fully replaces the machine-wide default
/// from `machine.toml [claude] flags`. The machine default itself is
/// `["--dangerously-skip-permissions"]` — fine inside the sandbox
/// because the container is the safety boundary; bypassing the
/// in-app permission UI is pure ergonomics.
fn resolve_claude_flags(
    project_cfg: &ConfigFile,
    machine_cfg: &claude_sandbox::machine::MachineConfig,
) -> Vec<String> {
    project_cfg
        .claude_flags
        .clone()
        .unwrap_or_else(|| machine_cfg.claude.flags.clone())
}

fn main() -> ExitCode {
    let argv0 = std::env::args().next().unwrap_or_default();
    let invoked = std::path::Path::new(&argv0)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "claude-sandbox".into());

    let result = if invoked == "cs" {
        run_cs()
    } else {
        run_host()
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            if matches!(&e, claude_sandbox::error::Error::ProjectNotFound(_)) {
                eprintln!(
                    "hint: run `claude-sandbox init` to mark this directory as a project, \
                     or `git init` to make it a git repo."
                );
            }
            ExitCode::FAILURE
        }
    }
}

fn run_host() -> Result<()> {
    let cli = HostCli::parse();
    logging::set_verbosity(cli.verbose);

    // `init` is the only command that runs without podman OR a project.
    if let Some(Cmd::Init) = &cli.command {
        let cwd = std::env::current_dir()?;
        let toml_path = cwd.join(".claude-sandbox.toml");
        if toml_path.exists() {
            println!("already initialized: {}", toml_path.display());
            return Ok(());
        }
        let name = derive_name(&cwd, &paths::home());
        cfg_edit::create_minimal(&toml_path, &name)?;
        println!("initialized: {}", toml_path.display());
        return Ok(());
    }

    // `cfg` is the interactive machine-setup wizard. Runs without podman
    // (nothing it does touches containers) and is the one command that
    // legitimately runs even when machine.toml doesn't exist — it's the
    // command that *creates* it.
    if let Some(Cmd::Cfg) = &cli.command {
        return run_cfg_wizard();
    }

    // Gate every other command on the machine-setup config existing.
    // Loud error with the next-step hint baked in.
    claude_sandbox::machine::require_setup_done()?;

    let podman = Podman::discover()?;

    // Commands that need podman but not a project.
    if let Some(cmd) = &cli.command {
        match cmd {
            Cmd::Ls { orphans, size } => return claude_sandbox::container::ls::ls(&podman, *orphans, *size),
            Cmd::Rebuild { recreate } => {
                claude_sandbox::podman::image::rebuild(&podman)?;
                if *recreate {
                    claude_sandbox::podman::image::recreate_all(&podman)?;
                }
                return Ok(());
            }
            _ => {}
        }
    }

    // Remaining commands need a project context.
    let cwd = std::env::current_dir()?;
    let project = find_project_root(&cwd)?;
    let derived_name = derive_name(&project, &paths::home());

    match cli.command.unwrap_or(Cmd::Start) {
        Cmd::Start => {
            if cli.main || cli.worktree.is_some() {
                return targeted_start(&podman, &project, &derived_name, "claude", cli.worktree.as_deref(), cli.force);
            }
            if !claude_sandbox::picker::has_worktrees(&project) {
                return start_or_shell(&podman, &project, &derived_name, "claude");
            }
            if cli.no_menu {
                return Err(claude_sandbox::error::Error::Other(
                    "menu would have shown but --no-menu was given".into(),
                ));
            }
            match claude_sandbox::picker::pick(&project)? {
                claude_sandbox::picker::Choice::Quit => Ok(()),
                claude_sandbox::picker::Choice::Main => start_or_shell(&podman, &project, &derived_name, "claude"),
                claude_sandbox::picker::Choice::Existing(w) => {
                    targeted_start(&podman, &project, &derived_name, "claude", Some(&w), cli.force)
                }
                claude_sandbox::picker::Choice::New(w, b) => {
                    create_worktree_and_start(&podman, &project, &derived_name, &w, b.as_deref())
                }
            }
        }
        Cmd::Shell => start_or_shell(&podman, &project, &derived_name, "bash"),
        Cmd::Goal { condition } => {
            let cond = condition.join(" ");
            start_goal(&podman, &project, &derived_name, &cond, cli.worktree.as_deref(), cli.force)
        }
        Cmd::Stop => {
            let cfg = load_cfg(&project)?;
            let resolved_name = cfg.name.clone().unwrap_or_else(|| derived_name.clone());
            lifecycle::stop(&podman, &resolved_name, &cfg.on_stop, &project)
        }
        Cmd::Down => {
            let resolved_name = load_cfg(&project)
                .ok()
                .and_then(|c| c.name)
                .unwrap_or_else(|| derived_name.clone());
            lifecycle::down(&podman, &resolved_name)
        }
        Cmd::Status => {
            let resolved_name = load_cfg(&project)
                .ok()
                .and_then(|c| c.name)
                .unwrap_or_else(|| derived_name.clone());
            let s = st::collect(&podman, &resolved_name)?;
            st::print(&s, &resolved_name);
            Ok(())
        }
        Cmd::Worktree { cmd } => {
            let resolved_name = load_cfg(&project)
                .ok()
                .and_then(|c| c.name)
                .unwrap_or_else(|| derived_name.clone());
            match cmd {
                claude_sandbox::cli::WorktreeCmd::Ls => {
                    ensure_running_if_exists(&podman, &resolved_name)?;
                    podman.run_inherit(&["exec".into(), resolved_name, "cs".into(), "worktree".into(), "ls".into()])
                }
                claude_sandbox::cli::WorktreeCmd::Rm { name: wt_name } => {
                    ensure_running_if_exists(&podman, &resolved_name)?;
                    podman.run_inherit(&[
                        "exec".into(),
                        resolved_name,
                        "cs".into(),
                        "worktree".into(),
                        "rm".into(),
                        wt_name,
                    ])
                }
            }
        }
        Cmd::Rename { new_name } => {
            let resolved_name = load_cfg(&project)
                .ok()
                .and_then(|c| c.name)
                .unwrap_or_else(|| derived_name.clone());
            claude_sandbox::container::rename::rename(&podman, &project, &resolved_name, &new_name)
        }
        Cmd::Migrate { new_path } => {
            let name = load_cfg(&project)
                .ok()
                .and_then(|c| c.name)
                .unwrap_or_else(|| derived_name.clone());
            claude_sandbox::container::migrate::migrate(&name, &new_path)
        }
        Cmd::Logs => {
            let resolved_name = load_cfg(&project)
                .ok()
                .and_then(|c| c.name)
                .unwrap_or_else(|| derived_name.clone());
            podman.run_inherit(&[
                "logs".into(),
                "--tail".into(),
                "200".into(),
                "--follow".into(),
                resolved_name,
            ])
        }
        Cmd::Ls { .. } | Cmd::Rebuild { .. } | Cmd::Init | Cmd::Cfg => unreachable!(),
    }
}

/// Interactive machine-setup wizard. Walks the user through each
/// host-environment value claude-sandbox needs to bake into the image
/// or apply at container-create time. Re-running the wizard pre-fills
/// each prompt with the currently-saved value (or the detected system
/// default for first-time setup).
///
/// Today: just the host UID. Future steps will land here in order
/// (SELinux, GPU vendor, image base, …).
fn run_cfg_wizard() -> Result<()> {
    use claude_sandbox::features::gpu::{self as gpu_feat, GpuVendor};
    use claude_sandbox::machine::{self, ClaudeSpec, GpuSpec, HostSpec, ImageSpec, MachineConfig};
    use dialoguer::{Confirm, Input, Password};

    let existing = if machine::exists() { machine::load().ok() } else { None };

    println!("==> claude-sandbox machine-setup wizard");
    println!("    Writes to: {}\n", machine::path().display());

    // ---- Host UID ----
    let detected_uid: u32 = nix::unistd::Uid::current().as_raw();
    let default_uid: u32 = existing.as_ref().map(|c| c.host.uid).unwrap_or(detected_uid);
    let label = if existing.is_some() {
        format!("host UID (current saved: {default_uid}, detected: {detected_uid})")
    } else {
        format!("host UID (detected: {detected_uid})")
    };
    let uid: u32 = Input::new()
        .with_prompt(label)
        .default(default_uid)
        .interact_text()
        .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;

    // ---- Base image (Dockerfile FROM line) ----
    let default_base: String = existing
        .as_ref()
        .map(|c| c.image.base.clone())
        .unwrap_or_else(|| ImageSpec::default().base);
    println!();
    println!(
        "    Base image: which OCI image the sandbox is built from. Must be apt-based"
    );
    println!(
        "    (Debian / Ubuntu / Mint). Other distros require editing assets/Dockerfile."
    );
    let base: String = Input::<String>::new()
        .with_prompt("base image")
        .default(default_base.clone())
        .interact_text()
        .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;

    // ---- Extra apt packages baked into the image ----
    let default_extras: Vec<String> = existing
        .as_ref()
        .map(|c| c.image.extra_packages.clone())
        .unwrap_or_else(|| ImageSpec::default().extra_packages);
    println!();
    println!(
        "    Extra apt packages: installed on top of the core set (ca-certificates"
    );
    println!(
        "    curl git sudo bash openssh-client acl pulseaudio-utils"
    );
    println!(
        "    sound-theme-freedesktop gnupg). Space-separated. Submit blank to"
    );
    println!(
        "    install nothing extra. Default reflects the project's opinionated set."
    );
    let raw_extras: String = Input::<String>::new()
        .with_prompt("extra packages")
        .default(default_extras.join(" "))
        .allow_empty(true)
        .interact_text()
        .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;
    let extra_packages: Vec<String> = raw_extras
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    // ---- GPU vendor ----
    let detected_vendor = gpu_feat::probe();
    let saved_vendor: Option<GpuVendor> = existing.as_ref().map(|c| c.gpu.vendor);
    let default_vendor = saved_vendor.unwrap_or(detected_vendor);
    let label = match (saved_vendor, detected_vendor) {
        (Some(sv), dv) if sv != dv => {
            format!("GPU vendor (saved: {}, probed: {}) [nvidia/amd/intel/none/custom]",
                    sv.as_str(), dv.as_str())
        }
        (Some(sv), _) => format!("GPU vendor (saved: {}) [nvidia/amd/intel/none/custom]", sv.as_str()),
        (None, dv) => format!("GPU vendor (probed: {}) [nvidia/amd/intel/none/custom]", dv.as_str()),
    };
    let vendor = loop {
        let raw: String = Input::<String>::new()
            .with_prompt(&label)
            .default(default_vendor.as_str().to_string())
            .interact_text()
            .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;
        match GpuVendor::parse(&raw) {
            Some(v) => break v,
            None => println!(
                "  ! unknown vendor `{raw}`. Pick one of: nvidia, amd, intel, none, custom"
            ),
        }
    };
    // extra_args is a power-user knob. Don't prompt; preserve existing,
    // default empty. Tell the user where to find it if they need it.
    let extra_args = existing
        .as_ref()
        .map(|c| c.gpu.extra_args.clone())
        .unwrap_or_default();
    if !extra_args.is_empty() {
        println!(
            "    Keeping existing gpu.extra_args ({} entries). Edit machine.toml to change.",
            extra_args.len()
        );
    } else {
        println!(
            "    (gpu.extra_args is empty. Edit machine.toml directly if your GPU needs extra podman flags.)"
        );
    }

    // ---- Default flags passed to `claude` on every launch ----
    //
    // The default is `--dangerously-skip-permissions`. That flag name
    // sounds scary on purpose — outside of containerised contexts,
    // letting claude run shell commands without prompts is a real risk.
    // INSIDE the rootless-Podman sandbox, the container itself is the
    // safety boundary: anything claude does is confined to the writable
    // layer + the bind-mounted dirs you opted into; it can't escape the
    // user namespace; sudo elevates only within the container; the host's
    // filesystem, packages, and other processes are unreachable. The
    // in-app permission prompts add friction without adding safety.
    //
    // You can append extra flags (e.g. `--model claude-opus-4-7`,
    // `--allowedTools Bash,Read,Edit`) or remove `--dangerously-skip-permissions`
    // entirely if you specifically want the in-app prompt UX back. The
    // list applies to `claude-sandbox` and `claude-sandbox goal`; per-project
    // `.claude-sandbox.toml` can override the list entirely.
    let default_claude_flags: Vec<String> = existing
        .as_ref()
        .map(|c| c.claude.flags.clone())
        .unwrap_or_else(|| ClaudeSpec::default().flags);
    println!();
    println!("    claude flags: passed to every `claude` invocation. Default");
    println!("    `--dangerously-skip-permissions` is fine inside the sandbox");
    println!("    (the container is the safety boundary; in-app prompts add");
    println!("    friction without protection). Space-separated; blank = none.");
    let raw_flags: String = Input::<String>::new()
        .with_prompt("claude flags")
        .default(default_claude_flags.join(" "))
        .allow_empty(true)
        .interact_text()
        .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;
    let claude_flags: Vec<String> = raw_flags
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    let new_cfg = MachineConfig {
        host: HostSpec { uid },
        image: ImageSpec { base, extra_packages },
        gpu: GpuSpec { vendor, extra_args },
        claude: ClaudeSpec { flags: claude_flags },
    };
    let machine_changed = existing.as_ref() != Some(&new_cfg);
    machine::save(&new_cfg)?;

    // ---- OAuth token (separate file from machine.toml) ----
    //
    // The token is a year-long, doesn't-rotate-on-use credential. Sharing
    // a single `.credentials.json` between host + multiple containers
    // triggers OAuth refresh-token rotation collisions; passing the token
    // via env var per-container sidesteps that entirely.
    let token_already = machine::oauth_token_exists();
    let want_token = if token_already {
        Confirm::new()
            .with_prompt("OAuth token already configured. Replace it?")
            .default(false)
            .interact()
            .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?
    } else {
        Confirm::new()
            .with_prompt(
                "Set up a long-lived OAuth token so containers don't share \
                 your auth file with the host? (recommended)",
            )
            .default(true)
            .interact()
            .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?
    };

    let oauth_changed = if want_token {
        println!();
        println!(
            "    Run `claude setup-token` in another terminal — it opens a browser,"
        );
        println!(
            "    walks you through OAuth, and prints a token starting with `sk-ant-oat01-`."
        );
        println!("    Paste it below (input is hidden):\n");
        let token: String = Password::new()
            .with_prompt("OAuth token")
            .interact()
            .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;
        let trimmed = token.trim();
        if !trimmed.starts_with("sk-ant-") {
            return Err(claude_sandbox::error::Error::Other(
                "that doesn't look like a Claude Code OAuth token \
                 (expected to start with `sk-ant-`). Aborting; existing token \
                 (if any) is unchanged."
                    .into(),
            ));
        }
        // Validate against Anthropic's API BEFORE saving — catches typos,
        // wrong tokens, and revoked tokens at the point the user pastes
        // them, instead of waiting until container start to surface the
        // problem. Network failures are demoted to a warning so an
        // offline laptop doesn't block the user from saving.
        println!("Validating token with Anthropic...");
        match machine::validate_oauth_token(trimmed) {
            machine::TokenValidation::Valid => {}
            machine::TokenValidation::Invalid { detail } => {
                return Err(claude_sandbox::error::Error::Other(format!(
                    "token rejected: {detail}. Generate a fresh one with \
                     `claude setup-token` and re-run `claude-sandbox cfg`."
                )));
            }
            machine::TokenValidation::Unknown { reason } => {
                eprintln!(
                    "[warn] couldn't verify token with Anthropic ({reason}). \
                     Saving anyway; container start will re-validate."
                );
            }
        }
        let prev_hash = machine::oauth_token_hash();
        machine::save_oauth_token(trimmed)?;
        let new_hash = machine::oauth_token_hash();
        prev_hash != new_hash
    } else {
        false
    };

    println!("\nSaved {}.", machine::path().display());
    if want_token {
        println!("Saved {}.", machine::oauth_token_path().display());
    }

    // ---- Desktop integration (KDE auto-install only) ----
    println!();
    run_cfg_desktop_step()?;

    // ---- Copy embedded defaults to ~/.config for editing ----
    println!();
    run_cfg_assets_step()?;

    if machine_changed || oauth_changed {
        println!();
        println!(
            "Configuration changed — existing containers will be auto-recreated on \
             next start (named home volume survives)."
        );
        if machine_changed {
            println!(
                "    Run `claude-sandbox rebuild` if you want to refresh the image now; \
                 otherwise the next `start` will trigger it."
            );
        }
    } else if existing.is_some() {
        println!("No changes.");
    }
    Ok(())
}

/// Detect the host's desktop environment and offer to install the
/// matching right-click "Open in claude-sandbox" context-menu entry.
/// KDE Plasma is auto-installable (we ship the Dolphin servicemenu);
/// other DEs need manual setup per `docs/recipes/context-menu.md`.
fn run_cfg_desktop_step() -> Result<()> {
    use claude_sandbox::desktop::{self, Desktop};
    use dialoguer::Confirm;

    match desktop::detect() {
        Desktop::Kde => {
            if desktop::kde_servicemenu_installed() {
                println!(
                    "==> Dolphin context menu already installed at {} — skipping.",
                    desktop::kde_servicemenu_path().display()
                );
                return Ok(());
            }
            println!("==> Desktop environment detected: KDE Plasma");
            let install = Confirm::new()
                .with_prompt(
                    "Install a Dolphin right-click \"Open in claude-sandbox\" \
                     context-menu entry?",
                )
                .default(true)
                .interact()
                .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;
            if install {
                let path = desktop::install_kde_servicemenu().map_err(|e| {
                    claude_sandbox::error::Error::Other(format!(
                        "writing servicemenu: {e}"
                    ))
                })?;
                println!("    Installed {}.", path.display());
                println!(
                    "    Edit the `Exec=` line if you want a terminal other than konsole."
                );
            }
        }
        Desktop::Other(de) => {
            println!(
                "==> Desktop environment detected: {de} (auto-install unsupported)"
            );
            println!(
                "    Only KDE Plasma has a bundled context-menu entry. See"
            );
            println!(
                "    docs/recipes/context-menu.md for the manual setup on GNOME,"
            );
            println!("    XFCE, Cinnamon, etc.");
        }
        Desktop::Unknown => {
            println!(
                "==> No desktop environment detected (XDG_CURRENT_DESKTOP unset)."
            );
            println!(
                "    Skipping context-menu setup. See docs/recipes/context-menu.md"
            );
            println!(
                "    if you want to wire one up manually on a non-standard setup."
            );
        }
    }
    Ok(())
}

/// Offer to drop editable copies of the embedded Dockerfile and
/// `config.toml` into `~/.config/claude-sandbox/`. Most users never need
/// this — the runtime three-tier lookup falls back to the package-shipped
/// (`/usr/share/claude-sandbox/`) or embedded versions — but users who
/// want to customise either file (extra Dockerfile RUN steps, global
/// project defaults) start here.
fn run_cfg_assets_step() -> Result<()> {
    use claude_sandbox::assets;
    use dialoguer::Confirm;

    let cfg_dir = paths::config_dir();
    let dockerfile = cfg_dir.join(assets::DOCKERFILE_NAME);
    let config = cfg_dir.join(assets::DEFAULT_CONFIG_NAME);
    let dockerfile_exists = dockerfile.exists();
    let config_exists = config.exists();

    if dockerfile_exists && config_exists {
        println!(
            "==> Editable copies already present at {} — skipping.",
            cfg_dir.display()
        );
        return Ok(());
    }

    println!(
        "==> Defaults source-of-truth lives at /usr/share/claude-sandbox/ (when packaged)"
    );
    println!(
        "    or is baked into this binary. To customise them, copy editable versions"
    );
    println!("    into {} now.", cfg_dir.display());
    if dockerfile_exists || config_exists {
        let present = if dockerfile_exists {
            assets::DOCKERFILE_NAME
        } else {
            assets::DEFAULT_CONFIG_NAME
        };
        println!("    ({} is already present; only the missing one will be written.)", present);
    }
    println!(
        "    --dangerously-skip-permissions is the default in the container because the"
    );
    println!(
        "    container itself is the safety boundary — bypassing Claude's in-app permission"
    );
    println!(
        "    UI is pure ergonomics, the host can't be damaged from inside."
    );

    let copy = Confirm::new()
        .with_prompt("Copy editable defaults into ~/.config/claude-sandbox/?")
        .default(false)
        .interact()
        .map_err(|e| claude_sandbox::error::Error::Other(format!("prompt failed: {e}")))?;
    if !copy {
        return Ok(());
    }

    let written = assets::populate_user_config(false).map_err(|e| {
        claude_sandbox::error::Error::Other(format!("populating ~/.config: {e}"))
    })?;
    if written.is_empty() {
        println!("    (Nothing copied — all targets already existed.)");
    } else {
        for p in written {
            println!("    Wrote {}.", p.display());
        }
    }
    Ok(())
}

/// Escape a string for single-quoted bash inclusion (`'foo'` form).
/// Necessary because the worktree launch path wraps the inner command in
/// `bash -c "..."` for trap/cleanup; the condition string must survive
/// the shell verbatim.
fn sh_squote(s: &str) -> String {
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

/// Run `claude -p --dangerously-skip-permissions /goal <condition>`
/// inside the project's container. Honors `-w worktree` like `start`.
///
/// `/goal` is a first-class Claude Code slash command that keeps the
/// agent looping turn-after-turn until a lightweight evaluator decides
/// the supplied condition is met. In `-p` (headless) mode it runs to
/// completion non-interactively — set-and-forget for long-lived goals.
fn start_goal(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
    condition: &str,
    worktree: Option<&str>,
    force: bool,
) -> Result<()> {
    match worktree {
        None => start_goal_main(podman, project, derived_name, condition),
        Some(w) => start_goal_in_worktree(podman, project, derived_name, w, condition, force),
    }
}

fn start_goal_main(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
    condition: &str,
) -> Result<()> {
    let prep = prepare_container(podman, project, derived_name)?;
    let flags = resolve_claude_flags(&prep.project_cfg, &prep.machine_cfg);
    let goal_arg = format!("/goal {condition}");
    let mut argv: Vec<&str> = vec!["claude", "-p"];
    argv.extend(flags.iter().map(|s| s.as_str()));
    argv.push(&goal_arg);
    claude_sandbox::terminal::set_title(project, None);
    exec_into(&prep.name, &argv)
}

fn start_goal_in_worktree(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
    worktree: &str,
    condition: &str,
    force: bool,
) -> Result<()> {
    use claude_sandbox::worktree::claim::{self, ClaimState};
    let prep = prepare_container(podman, project, derived_name)?;
    let flags = resolve_claude_flags(&prep.project_cfg, &prep.machine_cfg);
    let wt_dir = project.join(".worktrees").join(worktree);
    if !wt_dir.exists() {
        podman.run_inherit(&[
            "exec".into(),
            prep.name.clone(),
            "cs".into(),
            "worktree".into(),
            "add".into(),
            worktree.into(),
        ])?;
    }
    match claim::evaluate(&wt_dir)? {
        ClaimState::Active(c) if !force => {
            return Err(claude_sandbox::error::Error::Other(format!(
                "worktree {} is in use by PID {} (--force to override)",
                worktree, c.host_pid
            )));
        }
        _ => {}
    }
    claim::write(&wt_dir)?;
    let goal_arg = format!("/goal {condition}");
    let inner_cmd = format!(
        "claude -p {flags_str} {goal}",
        flags_str = flags.join(" "),
        goal = sh_squote(&goal_arg),
    );
    let wt_path = wt_dir.display().to_string();
    let cleanup = format!(
        "trap 'rm -f {wt}/.cs-session' EXIT INT TERM; cd {wt} && exec {inner_cmd}",
        wt = wt_path,
    );
    claude_sandbox::terminal::set_title(project, Some(worktree));
    claude_sandbox::container::exec::exec_into(&prep.name, &["bash", "-c", &cleanup])
}

/// Auto-create the toml if missing, load + merge config, resolve the
/// container name (honoring `name = "..."` overrides and registry-based
/// collision suffixing), create the container if missing, run setup +
/// deps on first create, ensure running, grant ACLs, run on_start hooks.
///
/// Returns the resolved container name. Used by both the main-checkout
/// launch path and the worktree launch path so they share container
/// lifecycle and only differ in their final exec.
pub struct Prepared {
    pub name: String,
    pub project_cfg: ConfigFile,
    pub machine_cfg: claude_sandbox::machine::MachineConfig,
}

fn prepare_container(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
) -> Result<Prepared> {
    use claude_sandbox::container::create::{
        ensure_container, grant_acls, run_deps_script, run_setup, CreateOptions,
    };
    use claude_sandbox::hooks;

    let toml_path = project.join(".claude-sandbox.toml");
    if !toml_path.exists() {
        claude_sandbox::step!("Initializing project marker (.claude-sandbox.toml)");
        cfg_edit::create_minimal(&toml_path, derived_name)?;
    }

    claude_sandbox::step!("Loading configuration");
    let cfg = load_global_merged(Some(&toml_path))?;
    let name = cfg.name.clone().unwrap_or_else(|| derived_name.to_string());

    // Machine-side check: if the local image's `cs-machine-hash` label
    // doesn't match the current `machine.toml` hash, rebuild the image
    // first. The image not existing at all also counts as a mismatch.
    // After rebuild, the existing toml-hash mechanism in ensure_container
    // will recreate any pre-existing container that was built against
    // the now-stale image.
    let machine_cfg = claude_sandbox::machine::require_setup_done()?;
    let current_machine_hash = claude_sandbox::machine::content_hash(&machine_cfg);
    let image_machine_hash = claude_sandbox::podman::image::image_machine_hash(podman);
    if image_machine_hash.as_deref() != Some(current_machine_hash.as_str()) {
        match image_machine_hash {
            None => claude_sandbox::step!(
                "Image missing or has no cs-machine-hash label — building"
            ),
            Some(_) => claude_sandbox::step!(
                "machine.toml changed since image was built — rebuilding image"
            ),
        }
        claude_sandbox::podman::image::rebuild(podman)?;
    }

    // Load the optional OAuth token (None = user hasn't run `cfg`'s token
    // step yet; container falls back to the bind-mounted .credentials.json
    // for auth). Hash always exists — empty-file sentinel covers absent.
    let oauth_token = claude_sandbox::machine::load_oauth_token()?;
    let current_oauth_hash = claude_sandbox::machine::oauth_token_hash();

    // If a token IS configured, verify it's still accepted by Anthropic
    // before we inject it into the container. Catches revocations / typos
    // upstream of the container's own auth failure. Unknown = network
    // issue → warn and proceed (we don't want to lock the user out of
    // their sandbox because Anthropic is having a 5xx moment).
    if let Some(tok) = oauth_token.as_deref() {
        claude_sandbox::step!("Validating OAuth token");
        match claude_sandbox::machine::validate_oauth_token(tok) {
            claude_sandbox::machine::TokenValidation::Valid => {}
            claude_sandbox::machine::TokenValidation::Invalid { detail } => {
                return Err(claude_sandbox::error::Error::Other(format!(
                    "OAuth token rejected by Anthropic ({detail}). \
                     Run `claude-sandbox cfg` to provide a fresh token."
                )));
            }
            claude_sandbox::machine::TokenValidation::Unknown { reason } => {
                eprintln!(
                    "[warn] couldn't verify OAuth token with Anthropic ({reason}); proceeding."
                );
            }
        }
    }

    let reg = claude_sandbox::registry::load()?;
    let resolved = match reg.entries.get(&name) {
        Some(existing_path) if existing_path != project => {
            let suffix = claude_sandbox::project::short_hash(project);
            let suffixed = format!("{name}-{suffix}");
            cfg_edit::set_name(&toml_path, &suffixed)?;
            suffixed
        }
        _ => name.clone(),
    };
    let name = resolved;

    let image = cfg.image.clone().unwrap_or_else(|| DEFAULT_IMAGE.into());

    let just_created = ensure_container(
        podman,
        &CreateOptions {
            name: &name,
            image: &image,
            project_path: project,
            config: &cfg,
            machine_hash: Some(&current_machine_hash),
            oauth_hash: Some(&current_oauth_hash),
            oauth_token: oauth_token.as_deref(),
            machine_cfg: Some(&machine_cfg),
        },
    )?;

    if just_created {
        if !cfg.setup.is_empty() {
            claude_sandbox::step!("Running setup hooks ({} step(s))", cfg.setup.len());
        }
        run_setup(podman, &name, project, &cfg.setup)?;
        // After setup hooks (which run once on create), re-apply the agent-
        // editable dependency manifest. This is what survives container reset:
        // agents append `apt install -y X` etc. to .claude-sandbox.deps.sh
        // and it auto-applies on next create without needing user intervention.
        if project.join(".claude-sandbox.deps.sh").exists() {
            claude_sandbox::step!("Running dependency install script (.claude-sandbox.deps.sh)");
        }
        run_deps_script(podman, &name, project)?;
    }

    lifecycle::ensure_running(podman, &name)?;
    // Idempotent: grant the non-root `claude` user write access to the
    // bind-mounted dirs. Cheap and safe to repeat every start.
    grant_acls(podman, &name, project, &cfg.mount)?;

    if !cfg.on_start.is_empty() {
        claude_sandbox::step!(
            "Running on_start hooks ({} step(s))",
            cfg.on_start.len()
        );
    }
    hooks::run(
        podman,
        &name,
        &cfg.on_start,
        &hooks::HookEnv {
            project_name: name.clone(),
            project_path: project.to_path_buf(),
            worktree_name: None,
        },
        false,
        hooks::HookUser::Root,
    )?;

    Ok(Prepared {
        name,
        project_cfg: cfg,
        machine_cfg,
    })
}

fn start_or_shell(podman: &Podman, project: &std::path::Path, derived_name: &str, inner: &str) -> Result<()> {
    let prep = prepare_container(podman, project, derived_name)?;
    let flags = resolve_claude_flags(&prep.project_cfg, &prep.machine_cfg);
    let mut argv: Vec<&str> = vec![inner];
    if inner == "claude" {
        argv.extend(flags.iter().map(|s| s.as_str()));
    }
    claude_sandbox::terminal::set_title(project, None);
    exec_into(&prep.name, &argv)
}

fn run_cs() -> Result<()> {
    use claude_sandbox::cli::{CsCmd, CsWorktreeCmd};
    use claude_sandbox::worktree::commands as wt;

    let cli = CsCli::parse();
    logging::set_verbosity(cli.verbose);
    // CS_PROJECT_PATH is set by the host wrapper at container create time
    // and points at the bind-mounted project root (same path inside and out).
    // Fall back to walking up from CWD looking for .claude-sandbox.toml, or
    // /work as a last resort for backward compatibility.
    let project = std::env::var_os("CS_PROJECT_PATH")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::current_dir().ok().and_then(|cwd| {
                let mut p: Option<&std::path::Path> = Some(&cwd);
                while let Some(d) = p {
                    if d.join(".claude-sandbox.toml").exists() {
                        return Some(d.to_path_buf());
                    }
                    p = d.parent();
                }
                None
            })
        })
        .unwrap_or_else(|| std::path::PathBuf::from("/work"));
    match cli.command {
        CsCmd::Status => {
            println!("project: {}", project.display());
            let cwd = std::env::current_dir()?;
            println!("worktree: {}", wt::current(&cwd, &project));
            Ok(())
        }
        CsCmd::Worktree { cmd } => match cmd {
            CsWorktreeCmd::Ls => {
                for w in wt::list(&project)? {
                    println!("{}\t{}\t{}", w.name, w.path.display(), w.branch.as_deref().unwrap_or("-"));
                }
                Ok(())
            }
            CsWorktreeCmd::Add { name, branch } => {
                let dir = wt::add(&project, &name, branch.as_deref())?;
                // Run worktree_setup hooks from /work/.claude-sandbox.toml.
                let cfg_path = project.join(".claude-sandbox.toml");
                if cfg_path.exists() {
                    let cfg = claude_sandbox::config::parse::load(&cfg_path)?;
                    run_in_dir_hooks(&dir, &name, &cfg.worktree_setup)?;
                }
                println!("{}", dir.display());
                Ok(())
            }
            CsWorktreeCmd::Rm { name } => wt::remove(&project, &name),
            CsWorktreeCmd::Current => {
                let cwd = std::env::current_dir()?;
                println!("{}", wt::current(&cwd, &project));
                Ok(())
            }
        },
        CsCmd::Goal { condition } => {
            let cond = condition.join(" ");
            let goal_arg = format!("/goal {cond}");
            // Use the flags the host wrapper baked in at container create.
            // Falls back to the safety baseline if a legacy container
            // (pre-CS_CLAUDE_FLAGS) is still around.
            let flags_str = std::env::var("CS_CLAUDE_FLAGS")
                .unwrap_or_else(|_| "--dangerously-skip-permissions".into());
            let mut argv: Vec<String> = vec!["claude".into(), "-p".into()];
            argv.extend(flags_str.split_whitespace().map(|s| s.to_string()));
            argv.push(goal_arg);
            let status = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .status()?;
            if !status.success() {
                return Err(claude_sandbox::error::Error::Other(format!(
                    "claude /goal exited {}",
                    status.code().unwrap_or(-1)
                )));
            }
            Ok(())
        }
        CsCmd::Apply => {
            let script = project.join(".claude-sandbox.deps.sh");
            if !script.exists() {
                eprintln!("no /work/.claude-sandbox.deps.sh — create it first");
                return Ok(());
            }
            // Re-run the deps script as root. We're already inside the
            // container; just sudo the bash invocation.
            let status = std::process::Command::new("sudo")
                .arg("bash")
                .arg(&script)
                .status()?;
            if !status.success() {
                return Err(claude_sandbox::error::Error::Other(format!(
                    "deps script exited {}",
                    status.code().unwrap_or(-1)
                )));
            }
            Ok(())
        }
    }
}

fn ensure_running_if_exists(podman: &Podman, name: &str) -> Result<()> {
    if !podman.container_exists(name)? {
        return Err(claude_sandbox::error::Error::Other(format!(
            "no container for this project; run `claude-sandbox start` first"
        )));
    }
    lifecycle::ensure_running(podman, name)
}

fn targeted_start(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
    inner: &str,
    worktree: Option<&str>,
    force: bool,
) -> Result<()> {
    match worktree {
        None => start_or_shell(podman, project, derived_name, inner),
        Some(w) => start_in_worktree(podman, project, derived_name, w, inner, force),
    }
}

fn start_in_worktree(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
    worktree: &str,
    inner: &str,
    force: bool,
) -> Result<()> {
    use claude_sandbox::worktree::claim::{self, ClaimState};
    // Full container lifecycle: create-if-missing, setup + deps on first
    // create, ensure running, ACLs, on_start hooks. Returns the resolved
    // (config-aware, collision-suffixed) container name + the configs
    // we'll need to assemble the claude argv.
    let prep = prepare_container(podman, project, derived_name)?;
    let flags = resolve_claude_flags(&prep.project_cfg, &prep.machine_cfg);
    let wt_dir = project.join(".worktrees").join(worktree);

    // Auto-create worktree if missing (spec §5.3: `-w feat-x` creates if absent).
    if !wt_dir.exists() {
        podman.run_inherit(&[
            "exec".into(),
            prep.name.clone(),
            "cs".into(),
            "worktree".into(),
            "add".into(),
            worktree.into(),
        ])?;
    }

    match claim::evaluate(&wt_dir)? {
        ClaimState::Active(c) if !force => {
            return Err(claude_sandbox::error::Error::Other(format!(
                "worktree {} is in use by PID {} (--force to override)",
                worktree, c.host_pid
            )));
        }
        _ => {}
    }
    claim::write(&wt_dir)?;
    // exec_into replaces the process; the claim survives until the wrapper exits.
    // We register a best-effort cleanup via a child-of-shell technique: wrap the exec in a sh -c
    // that removes the claim file after `claude` exits.
    //
    // Use `bash -c` (non-login). A login shell sources /etc/profile which
    // resets PATH and drops /root/.local/bin — that hides the `claude`
    // binary installed there by the Anthropic installer.
    let inner_cmd = if inner == "claude" {
        format!("claude {}", flags.join(" "))
    } else {
        inner.to_string()
    };
    // Worktree path is identical inside and outside (project bound at its
    // host absolute path).
    let wt_path = wt_dir.display().to_string();
    let cleanup = format!(
        "trap 'rm -f {wt}/.cs-session' EXIT INT TERM; cd {wt} && exec {inner_cmd}",
        wt = wt_path,
    );
    claude_sandbox::terminal::set_title(project, Some(worktree));
    claude_sandbox::container::exec::exec_into(&prep.name, &["bash", "-c", &cleanup])
}

fn create_worktree_and_start(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
    worktree: &str,
    branch: Option<&str>,
) -> Result<()> {
    use claude_sandbox::container::exec::exec_into;
    let prep = prepare_container(podman, project, derived_name)?;
    let flags = resolve_claude_flags(&prep.project_cfg, &prep.machine_cfg).join(" ");
    let mut args: Vec<String> = vec![
        "exec".into(),
        prep.name.clone(),
        "cs".into(),
        "worktree".into(),
        "add".into(),
        worktree.into(),
    ];
    if let Some(b) = branch {
        args.push("--branch".into());
        args.push(b.into());
    }
    podman.run_inherit(&args)?;
    let _ = project;
    // Now exec into claude in that worktree.
    // `bash -c` (non-login) preserves PATH; `-lc` would drop /root/.local/bin.
    let wt_path = project
        .join(".worktrees")
        .join(worktree)
        .display()
        .to_string();
    claude_sandbox::terminal::set_title(project, Some(worktree));
    exec_into(
        &prep.name,
        &[
            "bash", "-c",
            &format!("cd {wt_path} && exec claude {flags}"),
        ],
    )
}

fn run_in_dir_hooks(dir: &std::path::Path, worktree_name: &str, hooks: &[String]) -> Result<()> {
    if hooks.is_empty() {
        return Ok(());
    }
    let script = hooks.join(" && ");
    let status = std::process::Command::new("bash")
        .arg("-c")
        .arg(&script)
        .current_dir(dir)
        .env("CS_WORKTREE_NAME", worktree_name)
        .status()?;
    if !status.success() {
        return Err(claude_sandbox::error::Error::Other(format!(
            "worktree_setup failed for {worktree_name}"
        )));
    }
    Ok(())
}
