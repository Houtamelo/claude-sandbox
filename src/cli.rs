use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "claude-sandbox", version, about = "Run Claude in a per-project sandbox")]
pub struct HostCli {
    /// Verbose: -v for debug, -vv for very-verbose
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    /// Worktree to attach to (defaults to picker / main)
    #[arg(short = 'w', long, global = true)]
    pub worktree: Option<String>,

    /// Force main checkout, skip picker
    #[arg(long, global = true, conflicts_with = "worktree")]
    pub main: bool,

    /// Refuse to show interactive menu
    #[arg(long, global = true)]
    pub no_menu: bool,

    /// Force takeover of an active claim
    #[arg(long, global = true)]
    pub force: bool,

    #[command(subcommand)]
    pub command: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
pub enum Cmd {
    /// Ensure container exists + running, then run `claude` inside
    Start,
    /// Same lifecycle but drop into bash
    Shell,
    /// Stop the container (preserves state)
    Stop,
    /// Destroy the container and named home volume
    Down,
    /// Show status of the current project's container
    Status,
    /// List all cs-* containers across all projects
    Ls {
        #[arg(long)]
        orphans: bool,
        #[arg(long)]
        size: bool,
    },
    /// Rebuild the base image
    Rebuild {
        #[arg(long)]
        recreate: bool,
    },
    /// Tail container logs
    Logs,
    /// Atomic rename of this project's container
    Rename { new_name: String },
    /// Re-associate an orphan container with a new path
    Migrate { new_path: std::path::PathBuf },
    /// Worktree management
    Worktree {
        #[command(subcommand)]
        cmd: WorktreeCmd,
    },
    /// Mark the current directory as a project by writing a minimal
    /// `.claude-sandbox.toml`. Idempotent (no-op if the file already exists).
    Init,
    /// Interactive machine-setup wizard. Walks through host-environment
    /// questions (UID first; more inputs added incrementally) and writes
    /// the answers to `~/.config/claude-sandbox/machine.toml`. Every
    /// other subcommand requires this to have been run at least once.
    Cfg,
    /// Launch claude in headless `/goal` mode. The agent keeps working
    /// turn-after-turn until a Haiku evaluator decides the condition is
    /// met. All trailing args are joined into the goal condition.
    ///
    ///   claude-sandbox goal "spec.md is implemented and all tests pass"
    Goal {
        /// The end-state condition. Joined with spaces if multiple args.
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        condition: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum WorktreeCmd {
    Ls,
    Rm { name: String },
}

#[derive(Parser, Debug)]
#[command(name = "cs", version, about = "Inside-container helper for claude-sandbox")]
pub struct CsCli {
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: CsCmd,
}

#[derive(Subcommand, Debug)]
pub enum CsCmd {
    Status,
    Worktree {
        #[command(subcommand)]
        cmd: CsWorktreeCmd,
    },
    /// Run the per-project dependency script (`.claude-sandbox.deps.sh`)
    /// against the current container. Auto-runs on container creation;
    /// use this when you've appended a line and want to install it now.
    Apply,
    /// Launch a headless `/goal` claude session in the current directory.
    /// Equivalent to running `claude -p --dangerously-skip-permissions
    /// "/goal <condition>"` directly.
    Goal {
        #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
        condition: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum CsWorktreeCmd {
    /// Create (or attach to) a worktree under `.worktrees/<name>`.
    ///
    /// - No flag: create a fresh branch named after the worktree.
    /// - `--branch X`: check out the existing branch X. Errors if
    ///   X doesn't exist.
    /// - `--new-branch X`: create a new branch named X (from HEAD).
    ///   Errors if X already exists.
    ///
    /// `--branch` and `--new-branch` are mutually exclusive.
    Add {
        name: String,
        #[arg(long, conflicts_with = "new_branch")]
        branch: Option<String>,
        #[arg(long = "new-branch", conflicts_with = "branch")]
        new_branch: Option<String>,
    },
    Ls,
    Rm { name: String },
    Current,
}
