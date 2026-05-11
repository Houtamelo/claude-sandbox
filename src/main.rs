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
            ExitCode::FAILURE
        }
    }
}

fn run_host() -> Result<()> {
    let cli = HostCli::parse();
    logging::set_verbosity(cli.verbose);
    let podman = Podman::discover()?;

    // Commands that don't require a project context.
    if let Some(cmd) = &cli.command {
        match cmd {
            Cmd::Init { force } => {
                let cfg_dir = claude_sandbox::paths::config_dir();
                std::fs::create_dir_all(&cfg_dir)?;
                let dockerfile = cfg_dir.join("Dockerfile");
                let config_toml = cfg_dir.join("config.toml");
                if !dockerfile.exists() || *force {
                    std::fs::write(&dockerfile, include_str!("../assets/Dockerfile"))?;
                    println!("wrote {}", dockerfile.display());
                }
                if !config_toml.exists() || *force {
                    std::fs::write(&config_toml, include_str!("../assets/default-config.toml"))?;
                    println!("wrote {}", config_toml.display());
                }
                return Ok(());
            }
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
            let s = st::collect(&podman, &derived_name)?;
            st::print(&s, &derived_name);
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
            podman.run_inherit(&[
                "logs".into(),
                "--tail".into(),
                "200".into(),
                "--follow".into(),
                derived_name.clone(),
            ])
        }
        Cmd::Ls { .. } | Cmd::Rebuild { .. } | Cmd::Init { .. } => unreachable!(),
    }
}

fn start_or_shell(podman: &Podman, project: &std::path::Path, derived_name: &str, inner: &str) -> Result<()> {
    let toml_path = project.join(".claude-sandbox.toml");

    if !toml_path.exists() {
        cfg_edit::create_minimal(&toml_path, derived_name)?;
    }

    let global = paths::config_dir().join("config.toml");
    let cfg = load_merged(Some(&global), Some(&toml_path))?;
    let name = cfg.name.clone().unwrap_or_else(|| derived_name.to_string());

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

    use claude_sandbox::container::create::{ensure_container, run_setup, CreateOptions};
    use claude_sandbox::hooks;

    let just_created = ensure_container(
        podman,
        &CreateOptions {
            name: &name,
            image: &image,
            project_path: project,
            config: &cfg,
        },
    )?;

    if just_created {
        run_setup(podman, &name, project, &cfg.setup)?;
    }

    lifecycle::ensure_running(podman, &name)?;

    let mut on_start_combined: Vec<String> =
        claude_sandbox::features::tailscale::on_start_commands(&cfg.tailscale, &name);
    on_start_combined.extend(cfg.on_start.iter().cloned());

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
    )?;

    exec_into(&name, &[inner])
}

fn run_cs() -> Result<()> {
    use claude_sandbox::cli::{CsCmd, CsWorktreeCmd};
    use claude_sandbox::worktree::commands as wt;

    let cli = CsCli::parse();
    logging::set_verbosity(cli.verbose);
    // Inside the container, /work is always the project root.
    let project = std::path::PathBuf::from("/work");
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
    container: &str,
    inner: &str,
    worktree: Option<&str>,
    force: bool,
) -> Result<()> {
    match worktree {
        None => start_or_shell(podman, project, container, inner),
        Some(w) => start_in_worktree(podman, project, container, w, inner, force),
    }
}

fn start_in_worktree(
    podman: &Podman,
    project: &std::path::Path,
    container: &str,
    worktree: &str,
    inner: &str,
    force: bool,
) -> Result<()> {
    use claude_sandbox::worktree::claim::{self, ClaimState};
    let wt_dir = project.join(".worktrees").join(worktree);
    ensure_running_if_exists(podman, container)?;

    // Auto-create worktree if missing (spec §5.3: `-w feat-x` creates if absent).
    if !wt_dir.exists() {
        podman.run_inherit(&[
            "exec".into(),
            container.into(),
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
    let cleanup = format!(
        "trap 'rm -f /work/.worktrees/{w}/.cs-session' EXIT INT TERM; cd /work/.worktrees/{w} && {inner}",
        w = worktree,
        inner = inner,
    );
    claude_sandbox::container::exec::exec_into(container, &["bash", "-lc", &cleanup])
}

fn create_worktree_and_start(
    podman: &Podman,
    project: &std::path::Path,
    container: &str,
    worktree: &str,
    branch: Option<&str>,
) -> Result<()> {
    use claude_sandbox::container::exec::exec_into;
    // Ensure container running first so cs is available.
    ensure_running_if_exists(podman, container)?;
    let mut args: Vec<String> = vec![
        "exec".into(),
        container.into(),
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
    exec_into(
        container,
        &[
            "bash", "-lc",
            &format!("cd /work/.worktrees/{} && claude", worktree),
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
