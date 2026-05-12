use std::process::ExitCode;

use clap::Parser;

use claude_sandbox::cli::{CsCli, Cmd, HostCli};
use claude_sandbox::container::{exec::exec_into, lifecycle, status as st};
use claude_sandbox::config::{edit as cfg_edit, load_merged, ConfigFile};
use claude_sandbox::error::Result;
use claude_sandbox::paths;
use claude_sandbox::podman::runner::Podman;
use claude_sandbox::project::{derive_name, find_project_root};
use claude_sandbox::logging;

const DEFAULT_IMAGE: &str = "claude-sandbox:0.1";

/// The point of the sandbox is to let claude run with no permission
/// prompts — the container is the safety boundary, not the prompt UI.
/// Passed on every `claude` invocation; ignored for `bash` (shell) launches.
const CLAUDE_FLAGS: &[&str] = &["--dangerously-skip-permissions"];

fn load_cfg(project: &std::path::Path) -> Result<ConfigFile> {
    let toml_path = project.join(".claude-sandbox.toml");
    let global = paths::config_dir().join("config.toml");
    load_merged(Some(&global), if toml_path.exists() { Some(&toml_path) } else { None })
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
    use claude_sandbox::machine::{self, HostSpec, MachineConfig};
    use dialoguer::Input;

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

    let new_cfg = MachineConfig { host: HostSpec { uid } };
    let changed = existing.as_ref() != Some(&new_cfg);
    machine::save(&new_cfg)?;

    println!("\nSaved {}.", machine::path().display());
    if changed {
        println!(
            "Configuration changed — run `claude-sandbox rebuild` to update the \
             image. Existing containers will be auto-recreated on next start \
             (named home volume survives)."
        );
    } else {
        println!("No changes.");
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
    let name = prepare_container(podman, project, derived_name)?;
    let goal_arg = format!("/goal {condition}");
    let mut argv: Vec<&str> = vec!["claude", "-p"];
    argv.extend_from_slice(CLAUDE_FLAGS);
    argv.push(&goal_arg);
    claude_sandbox::terminal::set_title(project, None);
    exec_into(&name, &argv)
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
    let container = prepare_container(podman, project, derived_name)?;
    let wt_dir = project.join(".worktrees").join(worktree);
    if !wt_dir.exists() {
        podman.run_inherit(&[
            "exec".into(),
            container.clone(),
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
        "claude -p {flags} {goal}",
        flags = CLAUDE_FLAGS.join(" "),
        goal = sh_squote(&goal_arg),
    );
    let wt_path = wt_dir.display().to_string();
    let cleanup = format!(
        "trap 'rm -f {wt}/.cs-session' EXIT INT TERM; cd {wt} && exec {inner_cmd}",
        wt = wt_path,
    );
    claude_sandbox::terminal::set_title(project, Some(worktree));
    claude_sandbox::container::exec::exec_into(&container, &["bash", "-c", &cleanup])
}

/// Auto-create the toml if missing, load + merge config, resolve the
/// container name (honoring `name = "..."` overrides and registry-based
/// collision suffixing), create the container if missing, run setup +
/// deps on first create, ensure running, grant ACLs, run on_start hooks.
///
/// Returns the resolved container name. Used by both the main-checkout
/// launch path and the worktree launch path so they share container
/// lifecycle and only differ in their final exec.
fn prepare_container(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
) -> Result<String> {
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
    let global = paths::config_dir().join("config.toml");
    let cfg = load_merged(Some(&global), Some(&toml_path))?;
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

    let mut on_start_combined: Vec<String> =
        claude_sandbox::features::tailscale::on_start_commands(&cfg.tailscale, &name);
    on_start_combined.extend(cfg.on_start.iter().cloned());

    if !on_start_combined.is_empty() {
        claude_sandbox::step!(
            "Running on_start hooks ({} step(s))",
            on_start_combined.len()
        );
    }
    hooks::run(
        podman,
        &name,
        &on_start_combined,
        &hooks::HookEnv {
            project_name: name.clone(),
            project_path: project.to_path_buf(),
            worktree_name: None,
        },
        false,
        hooks::HookUser::Root,
    )?;

    Ok(name)
}

fn start_or_shell(podman: &Podman, project: &std::path::Path, derived_name: &str, inner: &str) -> Result<()> {
    let name = prepare_container(podman, project, derived_name)?;
    let mut argv: Vec<&str> = vec![inner];
    if inner == "claude" {
        argv.extend_from_slice(CLAUDE_FLAGS);
    }
    claude_sandbox::terminal::set_title(project, None);
    exec_into(&name, &argv)
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
            let mut argv: Vec<&str> = vec!["claude", "-p"];
            argv.extend_from_slice(CLAUDE_FLAGS);
            argv.push(&goal_arg);
            let status = std::process::Command::new(argv[0])
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
    // (config-aware, collision-suffixed) container name.
    let container = prepare_container(podman, project, derived_name)?;
    let wt_dir = project.join(".worktrees").join(worktree);

    // Auto-create worktree if missing (spec §5.3: `-w feat-x` creates if absent).
    if !wt_dir.exists() {
        podman.run_inherit(&[
            "exec".into(),
            container.clone(),
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
        format!("claude {}", CLAUDE_FLAGS.join(" "))
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
    claude_sandbox::container::exec::exec_into(&container, &["bash", "-c", &cleanup])
}

fn create_worktree_and_start(
    podman: &Podman,
    project: &std::path::Path,
    derived_name: &str,
    worktree: &str,
    branch: Option<&str>,
) -> Result<()> {
    use claude_sandbox::container::exec::exec_into;
    let container = prepare_container(podman, project, derived_name)?;
    let mut args: Vec<String> = vec![
        "exec".into(),
        container.clone(),
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
    let flags = CLAUDE_FLAGS.join(" ");
    let wt_path = project
        .join(".worktrees")
        .join(worktree)
        .display()
        .to_string();
    claude_sandbox::terminal::set_title(project, Some(worktree));
    exec_into(
        &container,
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
