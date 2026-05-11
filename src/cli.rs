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
}

#[derive(Subcommand, Debug)]
pub enum CsWorktreeCmd {
    Add { name: String, #[arg(long)] branch: Option<String> },
    Ls,
    Rm { name: String },
    Current,
}
