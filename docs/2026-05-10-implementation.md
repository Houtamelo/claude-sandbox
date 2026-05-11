# claude-sandbox Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a working `claude-sandbox` Rust binary installed at `~/.local/bin/`, with a buildable base image, that implements every feature in `docs/2026-05-10-design.md`.

**Architecture:** Single-binary Rust CLI dispatched on `argv[0]` (`claude-sandbox` host mode, `cs` in-container mode). Synchronous, no async runtime. Shells out to `podman` for all container operations; pure functions construct podman argument vectors so they can be unit-tested without a podman daemon. Config (`.claude-sandbox.toml`) is parsed with `serde`/`toml` and round-trip-mutated with `toml_edit`. Hooks are `bash -c` invocations inside the container. Worktree picker uses `dialoguer`. Process replacement uses `nix::unistd::execvp` so signals (Ctrl-C) and the TTY pass through cleanly to the inner `claude` / `bash`.

**Tech Stack:** Rust 2024 edition, `clap`, `serde`, `toml`, `toml_edit`, `dialoguer`, `nix`, `which`, `dirs`, `anyhow`, `thiserror`. `podman` ≥ 4.0 on the host. Target host: openSUSE Tumbleweed. Container OS: Debian bookworm-slim.

---

## File structure

Repository root is `~/Documents/projects/claude-sandbox/`. Layout produced by this plan:

```
Cargo.toml
Cargo.lock
Makefile
README.md
.gitignore
docs/
  2026-05-10-design.md            (already exists)
  2026-05-10-implementation.md    (this file)
assets/
  Dockerfile                      base image recipe, installed to ~/.config/claude-sandbox/Dockerfile
  default-config.toml             global defaults, installed to ~/.config/claude-sandbox/config.toml
src/
  main.rs                         entry: dispatch on argv[0]
  cli.rs                          clap definitions (host + cs modes)
  error.rs                        crate Error type
  logging.rs                      -v / -vv stderr logger
  paths.rs                        ~ and $VAR expansion, XDG dirs
  project.rs                      project root discovery, naming
  config/
    mod.rs                        ConfigFile struct, defaults, merge
    parse.rs                      serde-based load + validate
    edit.rs                       toml_edit mutations (auto-create, set name)
  podman/
    mod.rs                        re-exports
    args.rs                       pure-function arg builders (tested)
    runner.rs                     shell-out execution + JSON parsing
    image.rs                      build / inspect / tag
  container/
    mod.rs                        orchestrator: ensure_exists, ensure_running
    create.rs                     `podman create` orchestrator
    lifecycle.rs                  start, stop, down
    status.rs                     status reporting
    ls.rs                         list cs-* containers
    rename.rs                     rename command
    migrate.rs                    migrate command
    exec.rs                       execvp into `podman exec`
  mounts.rs                       Mount struct, default mounts, collision check
  env.rs                          Env build, passthrough resolution
  network.rs                      Port spec parsing, port-shift probe, ssh-agent
  features/
    tailscale.rs                  tailscale opt-in
    gpu.rs                        gpu opt-in
  hooks.rs                        hook executor (bash -c inside container)
  registry.rs                     ~/.local/share/claude-sandbox/registry.json
  worktree/
    mod.rs                        worktree types
    commands.rs                   cs worktree {add, ls, rm, current}
    claim.rs                      .cs-session claim file
  picker.rs                       dialoguer worktree picker
tests/
  common/mod.rs                   test helpers: tmpdir, fake-podman shim
  naming.rs                       path -> container name
  project_discovery.rs            walk-up logic
  config_parse.rs                 toml load + validate
  config_auto_create.rs           first-run toml creation
  config_edit.rs                  toml_edit round-trip
  podman_args.rs                  arg-vector construction
  mounts.rs                       default + extra mount construction
  ports.rs                        port spec parsing + shift probe
  registry.rs                     registry read/write
  claim.rs                        claim file lifecycle
  cli_smoke.rs                    `claude-sandbox --help`, subcommands exist
  integration_podman.rs           opt-in tests against real podman (gated by env)
```

Each file has one clear responsibility. Pure logic (arg construction, parsing, port probing, naming) sits in modules that can be unit-tested without podman; the side-effecting wrappers are thin.

---

## Phase 0 — Project skeleton

### Task 0.1: Cargo init and dependencies

**Files:**
- Create: `~/Documents/projects/claude-sandbox/Cargo.toml`
- Create: `~/Documents/projects/claude-sandbox/.gitignore`
- Create: `~/Documents/projects/claude-sandbox/src/main.rs`

- [ ] **Step 1: Initialize Cargo project (manually so we control layout)**

Create `Cargo.toml`:

```toml
[package]
name = "claude-sandbox"
version = "0.1.0"
edition = "2024"
description = "Run Claude Code in a rootless-Podman per-project sandbox."
license = "MIT"

[[bin]]
name = "claude-sandbox"
path = "src/main.rs"

[profile.release]
strip = "symbols"
lto = "thin"

[dependencies]
clap = { version = "4", features = ["derive", "wrap_help"] }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
toml_edit = "0.22"
dialoguer = { version = "0.11", default-features = false }
nix = { version = "0.29", features = ["process"] }
which = "6"
dirs = "5"
anyhow = "1"
thiserror = "1"
serde_json = "1"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

Create `.gitignore`:

```
/target
**/*.rs.bk
Cargo.lock.bak
```

Create stub `src/main.rs`:

```rust
fn main() {
    println!("claude-sandbox: stub");
}
```

- [ ] **Step 2: Verify it builds**

Run: `cd ~/Documents/projects/claude-sandbox && cargo build`
Expected: builds successfully, produces `target/debug/claude-sandbox`.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore src/main.rs
git commit -m "scaffold: cargo project with dependency baseline"
```

### Task 0.2: Module skeleton with stubs

**Files:**
- Modify: `src/main.rs`
- Create: `src/cli.rs`, `src/error.rs`, `src/logging.rs`, `src/paths.rs`, `src/project.rs`
- Create: `src/config/mod.rs`, `src/podman/mod.rs`, `src/container/mod.rs`, `src/worktree/mod.rs`, `src/features/mod.rs`
- Create: empty children `src/podman/args.rs`, `src/podman/runner.rs`, `src/podman/image.rs`, `src/container/{create,lifecycle,status,ls,rename,migrate,exec}.rs`, `src/worktree/{commands,claim}.rs`, `src/features/{tailscale,gpu}.rs`, `src/config/{parse,edit}.rs`, `src/mounts.rs`, `src/env.rs`, `src/network.rs`, `src/hooks.rs`, `src/registry.rs`, `src/picker.rs`

- [ ] **Step 1: Create the directory tree and empty module files**

Run from repo root:

```bash
mkdir -p src/config src/podman src/container src/worktree src/features
touch src/cli.rs src/error.rs src/logging.rs src/paths.rs src/project.rs \
      src/mounts.rs src/env.rs src/network.rs src/hooks.rs src/registry.rs src/picker.rs
touch src/config/mod.rs src/config/parse.rs src/config/edit.rs
touch src/podman/mod.rs src/podman/args.rs src/podman/runner.rs src/podman/image.rs
touch src/container/mod.rs src/container/create.rs src/container/lifecycle.rs src/container/status.rs \
      src/container/ls.rs src/container/rename.rs src/container/migrate.rs src/container/exec.rs
touch src/worktree/mod.rs src/worktree/commands.rs src/worktree/claim.rs
touch src/features/mod.rs src/features/tailscale.rs src/features/gpu.rs
```

- [ ] **Step 2: Declare modules in main.rs**

Replace `src/main.rs`:

```rust
mod cli;
mod config;
mod container;
mod env;
mod error;
mod features;
mod hooks;
mod logging;
mod mounts;
mod network;
mod paths;
mod picker;
mod podman;
mod project;
mod registry;
mod worktree;

fn main() {
    println!("claude-sandbox: stub");
}
```

- [ ] **Step 3: Add module declarations inside `mod.rs` files**

`src/config/mod.rs`:

```rust
pub mod edit;
pub mod parse;
```

`src/podman/mod.rs`:

```rust
pub mod args;
pub mod image;
pub mod runner;
```

`src/container/mod.rs`:

```rust
pub mod create;
pub mod exec;
pub mod lifecycle;
pub mod ls;
pub mod migrate;
pub mod rename;
pub mod status;
```

`src/worktree/mod.rs`:

```rust
pub mod claim;
pub mod commands;
```

`src/features/mod.rs`:

```rust
pub mod gpu;
pub mod tailscale;
```

- [ ] **Step 4: Verify it builds clean**

Run: `cargo build`
Expected: builds with warnings about unused modules (expected; ignore).

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "scaffold: module skeleton"
```

### Task 0.3: Crate-wide error type and logger

**Files:**
- Modify: `src/error.rs`, `src/logging.rs`

- [ ] **Step 1: Define the error type**

`src/error.rs`:

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("config error: {0}")]
    Config(String),

    #[error("podman error: {0}")]
    Podman(String),

    #[error("project not found: no .claude-sandbox.toml or .git ancestor of {0}")]
    ProjectNotFound(std::path::PathBuf),

    #[error("name collision: '{0}' is already used by container at {1}")]
    NameCollision(String, std::path::PathBuf),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;
```

- [ ] **Step 2: Define the logger**

`src/logging.rs`:

```rust
use std::sync::atomic::{AtomicU8, Ordering};

static VERBOSITY: AtomicU8 = AtomicU8::new(0);

pub fn set_verbosity(level: u8) {
    VERBOSITY.store(level, Ordering::Relaxed);
}

pub fn verbosity() -> u8 {
    VERBOSITY.load(Ordering::Relaxed)
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        eprintln!("{}", format!($($arg)*));
    };
}

#[macro_export]
macro_rules! debug1 {
    ($($arg:tt)*) => {
        if $crate::logging::verbosity() >= 1 {
            eprintln!("[debug] {}", format!($($arg)*));
        }
    };
}

#[macro_export]
macro_rules! debug2 {
    ($($arg:tt)*) => {
        if $crate::logging::verbosity() >= 2 {
            eprintln!("[debug2] {}", format!($($arg)*));
        }
    };
}
```

- [ ] **Step 3: Build clean**

Run: `cargo build`
Expected: builds with only "unused" warnings.

- [ ] **Step 4: Commit**

```bash
git add src/error.rs src/logging.rs
git commit -m "scaffold: error type and verbosity-gated logger"
```

---

## Phase 1 — Bare wrapper

This phase delivers a functional `claude-sandbox` that can `start`, `shell`, `stop`, `down`, and `status` a hardcoded-image container. No config file yet. The image is `claude-sandbox:0.1`, assumed pre-built.

### Task 1.1: Container name derivation (TDD)

**Files:**
- Modify: `src/project.rs`
- Test: `tests/naming.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/naming.rs`:

```rust
use std::path::PathBuf;

use claude_sandbox::project::derive_name;

#[test]
fn name_under_home_uses_relative_components() {
    let home = PathBuf::from("/home/user");
    let path = PathBuf::from("/home/user/Documents/projects/spire");
    assert_eq!(derive_name(&path, &home), "documents-projects-spire");
}

#[test]
fn name_outside_home_uses_root_prefix() {
    let home = PathBuf::from("/home/user");
    let path = PathBuf::from("/srv/repos/spire");
    assert_eq!(derive_name(&path, &home), "root-srv-repos-spire");
}

#[test]
fn whitespace_collapses_to_dash() {
    let home = PathBuf::from("/home/user");
    let path = PathBuf::from("/home/user/My Projects/Cool Tool");
    assert_eq!(derive_name(&path, &home), "my-projects-cool-tool");
}

#[test]
fn home_itself_is_just_home() {
    let home = PathBuf::from("/home/user");
    assert_eq!(derive_name(&home, &home), "home");
}
```

For the test to compile, expose the crate as a library too. **Add to `Cargo.toml`** (under `[package]`):

```toml
[lib]
name = "claude_sandbox"
path = "src/lib.rs"
```

Create `src/lib.rs` mirroring main's module declarations:

```rust
pub mod cli;
pub mod config;
pub mod container;
pub mod env;
pub mod error;
pub mod features;
pub mod hooks;
pub mod logging;
pub mod mounts;
pub mod network;
pub mod paths;
pub mod picker;
pub mod podman;
pub mod project;
pub mod registry;
pub mod worktree;
```

Update `src/main.rs` to use the library:

```rust
fn main() {
    println!("claude-sandbox: stub");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test naming`
Expected: compile error — `derive_name` not found.

- [ ] **Step 3: Implement `derive_name`**

`src/project.rs`:

```rust
use std::path::Path;

pub fn derive_name(path: &Path, home: &Path) -> String {
    if path == home {
        return "home".to_string();
    }
    let relative_components: Vec<String> = if let Ok(rel) = path.strip_prefix(home) {
        rel.components()
            .map(|c| normalize_component(&c.as_os_str().to_string_lossy()))
            .collect()
    } else {
        // outside HOME -> "root" + absolute path components
        std::iter::once("root".to_string())
            .chain(
                path.components()
                    .filter(|c| c.as_os_str() != "/")
                    .map(|c| normalize_component(&c.as_os_str().to_string_lossy())),
            )
            .collect()
    };
    relative_components.join("-")
}

fn normalize_component(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| if c.is_whitespace() || c == '/' { '-' } else { c })
        .collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test naming`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml src/lib.rs src/main.rs src/project.rs tests/naming.rs
git commit -m "feat(project): derive container name from path"
```

### Task 1.2: Project root discovery (TDD)

**Files:**
- Modify: `src/project.rs`
- Test: `tests/project_discovery.rs`

- [ ] **Step 1: Write the failing test**

Create `tests/project_discovery.rs`:

```rust
use std::fs;

use tempfile::tempdir;

use claude_sandbox::project::find_project_root;

#[test]
fn finds_dir_with_toml() {
    let tmp = tempdir().unwrap();
    let proj = tmp.path().join("p");
    let sub = proj.join("a/b/c");
    fs::create_dir_all(&sub).unwrap();
    fs::write(proj.join(".claude-sandbox.toml"), "name = \"p\"\n").unwrap();

    assert_eq!(find_project_root(&sub).unwrap(), proj);
}

#[test]
fn finds_dir_with_git() {
    let tmp = tempdir().unwrap();
    let proj = tmp.path().join("p");
    let sub = proj.join("a/b");
    fs::create_dir_all(&sub).unwrap();
    fs::create_dir(proj.join(".git")).unwrap();

    assert_eq!(find_project_root(&sub).unwrap(), proj);
}

#[test]
fn toml_wins_over_git_when_closer() {
    let tmp = tempdir().unwrap();
    let outer = tmp.path().join("outer");
    let inner = outer.join("inner");
    let cwd = inner.join("sub");
    fs::create_dir_all(&cwd).unwrap();
    fs::create_dir(outer.join(".git")).unwrap();
    fs::write(inner.join(".claude-sandbox.toml"), "").unwrap();

    assert_eq!(find_project_root(&cwd).unwrap(), inner);
}

#[test]
fn errors_when_no_marker_anywhere() {
    let tmp = tempdir().unwrap();
    let sub = tmp.path().join("a/b");
    fs::create_dir_all(&sub).unwrap();

    assert!(find_project_root(&sub).is_err());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test project_discovery`
Expected: compile error — `find_project_root` not found.

- [ ] **Step 3: Implement**

Append to `src/project.rs`:

```rust
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

pub fn find_project_root(start: &Path) -> Result<PathBuf> {
    let mut cur: Option<&Path> = Some(start);
    while let Some(p) = cur {
        if p.join(".claude-sandbox.toml").exists() {
            return Ok(p.to_path_buf());
        }
        if p.join(".git").exists() {
            return Ok(p.to_path_buf());
        }
        cur = p.parent();
    }
    Err(Error::ProjectNotFound(start.to_path_buf()))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --test project_discovery`
Expected: 4 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/project.rs tests/project_discovery.rs
git commit -m "feat(project): walk-up project root discovery"
```

### Task 1.3: Podman arg-vector builder (TDD)

The orchestration layer constructs argv vectors as pure functions, then a thin runner shells out. This is the seam that makes everything testable without podman.

**Files:**
- Modify: `src/podman/args.rs`, `src/mounts.rs`
- Test: `tests/podman_args.rs`

- [ ] **Step 1: Define `Mount` and the `CreateSpec` types**

`src/mounts.rs`:

```rust
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    pub host: PathBuf,
    pub container: PathBuf,
    pub ro: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Volume {
    Bind(Mount),
    Named { name: String, container: PathBuf, ro: bool },
}
```

`src/podman/args.rs`:

```rust
use std::path::Path;

use crate::mounts::Volume;

#[derive(Debug, Clone)]
pub struct CreateSpec<'a> {
    pub name: &'a str,
    pub image: &'a str,
    pub volumes: &'a [Volume],
    pub env: &'a [(String, String)],
    pub network: &'a str,
    pub ports: &'a [PortMapping],
    pub workdir: &'a Path,
    pub extra: &'a [String],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMapping {
    pub host: u16,
    pub container: u16,
}

pub fn create_args(spec: &CreateSpec) -> Vec<String> {
    let mut v: Vec<String> = vec![
        "create".into(),
        "--name".into(),
        spec.name.into(),
        "--workdir".into(),
        spec.workdir.display().to_string(),
        "--network".into(),
        spec.network.into(),
        "--init".into(),
    ];
    for vol in spec.volumes {
        v.push("--volume".into());
        v.push(volume_arg(vol));
    }
    for (k, val) in spec.env {
        v.push("--env".into());
        v.push(format!("{}={}", k, val));
    }
    for p in spec.ports {
        v.push("--publish".into());
        v.push(format!("{}:{}", p.host, p.container));
    }
    v.extend(spec.extra.iter().cloned());
    v.push(spec.image.into());
    v.push("sleep".into());
    v.push("infinity".into());
    v
}

fn volume_arg(vol: &Volume) -> String {
    match vol {
        Volume::Bind(m) => format!(
            "{}:{}{}",
            m.host.display(),
            m.container.display(),
            if m.ro { ":ro" } else { "" }
        ),
        Volume::Named { name, container, ro } => format!(
            "{}:{}{}",
            name,
            container.display(),
            if *ro { ":ro" } else { "" }
        ),
    }
}

pub fn start_args(name: &str) -> Vec<String> {
    vec!["start".into(), name.into()]
}

pub fn stop_args(name: &str) -> Vec<String> {
    vec!["stop".into(), name.into()]
}

pub fn rm_args(name: &str) -> Vec<String> {
    vec!["rm".into(), "--force".into(), "--volumes".into(), name.into()]
}

pub fn exec_args(name: &str, interactive: bool, cmd: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = vec!["exec".into()];
    if interactive {
        v.push("-it".into());
    }
    v.push(name.into());
    v.extend(cmd.iter().map(|s| (*s).into()));
    v
}

pub fn inspect_args(name: &str) -> Vec<String> {
    vec!["inspect".into(), "--format".into(), "{{json .}}".into(), name.into()]
}
```

- [ ] **Step 2: Write the failing test**

Create `tests/podman_args.rs`:

```rust
use std::path::PathBuf;

use claude_sandbox::mounts::{Mount, Volume};
use claude_sandbox::podman::args::{create_args, exec_args, CreateSpec, PortMapping};

#[test]
fn create_args_baseline() {
    let vols = vec![
        Volume::Bind(Mount {
            host: PathBuf::from("/home/u/p"),
            container: PathBuf::from("/work"),
            ro: false,
        }),
        Volume::Named {
            name: "cs-p-home".into(),
            container: PathBuf::from("/root"),
            ro: false,
        },
    ];
    let env = vec![("FOO".to_string(), "bar".to_string())];
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "cs-p",
        image: "claude-sandbox:0.1",
        volumes: &vols,
        env: &env,
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
    };

    let args = create_args(&spec);
    assert_eq!(args[0], "create");
    assert!(args.contains(&"--name".into()));
    assert!(args.contains(&"cs-p".into()));
    assert!(args.contains(&"--volume".into()));
    assert!(args.contains(&"/home/u/p:/work".into()));
    assert!(args.contains(&"cs-p-home:/root".into()));
    assert!(args.contains(&"FOO=bar".into()));
    assert!(args.contains(&"claude-sandbox:0.1".into()));
    assert!(args.contains(&"sleep".into()));
    assert!(args.contains(&"infinity".into()));
}

#[test]
fn create_args_with_ports_and_ro_mount() {
    let vols = vec![Volume::Bind(Mount {
        host: PathBuf::from("/etc/foo"),
        container: PathBuf::from("/etc/foo"),
        ro: true,
    })];
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &vols,
        env: &[],
        network: "bridge",
        ports: &[PortMapping { host: 5173, container: 5173 }],
        workdir: &workdir,
        extra: &[],
    };
    let args = create_args(&spec);
    assert!(args.contains(&"/etc/foo:/etc/foo:ro".into()));
    assert!(args.contains(&"--publish".into()));
    assert!(args.contains(&"5173:5173".into()));
}

#[test]
fn exec_args_passes_command() {
    let args = exec_args("c", true, &["claude"]);
    assert_eq!(args, vec!["exec", "-it", "c", "claude"]);
}
```

- [ ] **Step 3: Run test to verify it passes**

Run: `cargo test --test podman_args`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/mounts.rs src/podman/args.rs tests/podman_args.rs
git commit -m "feat(podman): pure-function arg builders for create/start/stop/rm/exec"
```

### Task 1.4: Podman runner (side-effecting wrapper)

**Files:**
- Modify: `src/podman/runner.rs`

- [ ] **Step 1: Implement the runner**

`src/podman/runner.rs`:

```rust
use std::process::{Command, Output, Stdio};

use serde_json::Value;

use crate::error::{Error, Result};
use crate::{debug1, debug2};

pub struct Podman {
    bin: std::path::PathBuf,
}

impl Podman {
    pub fn discover() -> Result<Self> {
        let bin = which::which("podman")
            .map_err(|_| Error::Podman("`podman` not found on PATH".into()))?;
        Ok(Self { bin })
    }

    pub fn run(&self, args: &[String]) -> Result<Output> {
        debug1!("podman {}", args.join(" "));
        let output = Command::new(&self.bin)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Podman(format!(
                "podman {} failed:\n{}",
                args.first().map(|s| s.as_str()).unwrap_or(""),
                stderr.trim()
            )));
        }
        debug2!(
            "podman stdout: {}",
            String::from_utf8_lossy(&output.stdout).trim()
        );
        Ok(output)
    }

    pub fn run_inherit(&self, args: &[String]) -> Result<()> {
        debug1!("podman {}", args.join(" "));
        let status = Command::new(&self.bin).args(args).status()?;
        if !status.success() {
            return Err(Error::Podman(format!(
                "podman {} exited {}",
                args.first().map(|s| s.as_str()).unwrap_or(""),
                status.code().unwrap_or(-1)
            )));
        }
        Ok(())
    }

    pub fn run_json(&self, args: &[String]) -> Result<Value> {
        let out = self.run(args)?;
        let s = String::from_utf8_lossy(&out.stdout);
        serde_json::from_str::<Value>(s.trim())
            .map_err(|e| Error::Podman(format!("invalid json from podman: {e}")))
    }

    pub fn container_exists(&self, name: &str) -> Result<bool> {
        let args = vec![
            "container".into(),
            "exists".into(),
            name.into(),
        ];
        debug1!("podman {}", args.join(" "));
        let status = Command::new(&self.bin).args(&args).status()?;
        Ok(status.success())
    }

    pub fn container_running(&self, name: &str) -> Result<bool> {
        if !self.container_exists(name)? {
            return Ok(false);
        }
        let v = self.run_json(&crate::podman::args::inspect_args(name))?;
        Ok(v.get("State")
            .and_then(|s| s.get("Running"))
            .and_then(|r| r.as_bool())
            .unwrap_or(false))
    }
}
```

- [ ] **Step 2: Build clean**

Run: `cargo build`
Expected: builds with unused warnings.

- [ ] **Step 3: Commit**

```bash
git add src/podman/runner.rs
git commit -m "feat(podman): runner with json + exists + running helpers"
```

### Task 1.5: Default mounts and ssh-agent forwarding

**Files:**
- Modify: `src/mounts.rs`, `src/network.rs`

- [ ] **Step 1: Implement default mounts**

Append to `src/mounts.rs`:

```rust
use std::path::Path;

use crate::paths;

pub fn default_volumes(project_path: &Path, container_name: &str) -> Vec<Volume> {
    let home = paths::home();
    let mut v = vec![
        Volume::Bind(Mount {
            host: project_path.to_path_buf(),
            container: PathBuf::from("/work"),
            ro: false,
        }),
        Volume::Bind(Mount {
            host: home.join(".claude"),
            container: PathBuf::from("/root/.claude"),
            ro: false,
        }),
        Volume::Named {
            name: format!("cs-{}-home", container_name),
            container: PathBuf::from("/root"),
            ro: false,
        },
    ];
    let gitconfig = home.join(".gitconfig");
    if gitconfig.exists() {
        v.push(Volume::Bind(Mount {
            host: gitconfig,
            container: PathBuf::from("/root/.gitconfig"),
            ro: true,
        }));
    }
    v
}
```

- [ ] **Step 2: Implement paths helper**

`src/paths.rs`:

```rust
use std::path::PathBuf;

pub fn home() -> PathBuf {
    dirs::home_dir().expect("HOME must be set")
}

pub fn config_dir() -> PathBuf {
    home().join(".config/claude-sandbox")
}

pub fn data_dir() -> PathBuf {
    home().join(".local/share/claude-sandbox")
}

pub fn expand(input: &str) -> String {
    let mut s = input.to_string();
    if let Some(rest) = s.strip_prefix("~/") {
        s = home().join(rest).display().to_string();
    } else if s == "~" {
        s = home().display().to_string();
    }
    expand_env(&s)
}

fn expand_env(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() {
            let mut end = i + 1;
            while end < bytes.len() && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_') {
                end += 1;
            }
            if end > i + 1 {
                let key = std::str::from_utf8(&bytes[i + 1..end]).unwrap();
                if let Ok(v) = std::env::var(key) {
                    out.push_str(&v);
                    i = end;
                    continue;
                }
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}
```

- [ ] **Step 3: Implement ssh-agent socket discovery**

`src/network.rs`:

```rust
use std::path::PathBuf;

pub fn ssh_agent_socket() -> Option<PathBuf> {
    std::env::var_os("SSH_AUTH_SOCK").map(PathBuf::from)
}
```

- [ ] **Step 4: Build clean**

Run: `cargo build`
Expected: builds.

- [ ] **Step 5: Commit**

```bash
git add src/mounts.rs src/paths.rs src/network.rs
git commit -m "feat(mounts): default volume set + ~ / $VAR expansion + ssh-agent socket"
```

### Task 1.6: Container lifecycle commands (no config yet)

**Files:**
- Modify: `src/container/create.rs`, `src/container/lifecycle.rs`, `src/container/exec.rs`, `src/container/status.rs`

- [ ] **Step 1: Implement create**

`src/container/create.rs`:

```rust
use std::path::Path;

use crate::error::Result;
use crate::mounts::{default_volumes, Volume};
use crate::network::ssh_agent_socket;
use crate::podman::args::{create_args, CreateSpec};
use crate::podman::runner::Podman;

pub struct CreateOptions<'a> {
    pub name: &'a str,
    pub image: &'a str,
    pub project_path: &'a Path,
    pub ssh_agent: bool,
}

pub fn ensure_container(podman: &Podman, opts: &CreateOptions) -> Result<()> {
    if podman.container_exists(opts.name)? {
        return Ok(());
    }
    let mut volumes = default_volumes(opts.project_path, opts.name);
    let mut env: Vec<(String, String)> = Vec::new();

    if opts.ssh_agent {
        if let Some(sock) = ssh_agent_socket() {
            volumes.push(Volume::Bind(crate::mounts::Mount {
                host: sock.clone(),
                container: std::path::PathBuf::from("/ssh-agent.sock"),
                ro: false,
            }));
            env.push(("SSH_AUTH_SOCK".into(), "/ssh-agent.sock".into()));
        }
    }

    let workdir = std::path::PathBuf::from("/work");
    let spec = CreateSpec {
        name: opts.name,
        image: opts.image,
        volumes: &volumes,
        env: &env,
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
    };
    podman.run(&create_args(&spec))?;
    Ok(())
}
```

- [ ] **Step 2: Implement start / stop / down**

`src/container/lifecycle.rs`:

```rust
use crate::error::Result;
use crate::podman::args::{rm_args, start_args, stop_args};
use crate::podman::runner::Podman;

pub fn ensure_running(podman: &Podman, name: &str) -> Result<()> {
    if !podman.container_running(name)? {
        podman.run(&start_args(name))?;
    }
    Ok(())
}

pub fn stop(podman: &Podman, name: &str) -> Result<()> {
    podman.run(&stop_args(name))?;
    Ok(())
}

pub fn down(podman: &Podman, name: &str) -> Result<()> {
    podman.run(&rm_args(name))?;
    let vol = format!("cs-{}-home", name);
    let _ = podman.run(&["volume".into(), "rm".into(), "--force".into(), vol]);
    Ok(())
}
```

- [ ] **Step 3: Implement exec (process replacement)**

`src/container/exec.rs`:

```rust
use std::ffi::CString;

use nix::unistd::execvp;

use crate::error::{Error, Result};

pub fn exec_into(name: &str, cmd: &[&str]) -> Result<()> {
    let argv = build_argv(name, cmd)?;
    let prog = CString::new("podman").unwrap();
    let cs: Vec<CString> = argv.iter().map(|s| CString::new(s.as_str()).unwrap()).collect();
    execvp(&prog, &cs).map_err(|e| Error::Other(format!("execvp failed: {e}")))?;
    unreachable!();
}

fn build_argv(name: &str, cmd: &[&str]) -> Result<Vec<String>> {
    let mut v = vec!["podman".to_string(), "exec".into(), "-it".into(), name.into()];
    v.extend(cmd.iter().map(|s| (*s).to_string()));
    Ok(v)
}
```

- [ ] **Step 4: Implement basic status**

`src/container/status.rs`:

```rust
use crate::error::Result;
use crate::podman::runner::Podman;

pub struct Status {
    pub exists: bool,
    pub running: bool,
}

pub fn collect(podman: &Podman, name: &str) -> Result<Status> {
    let exists = podman.container_exists(name)?;
    let running = if exists { podman.container_running(name)? } else { false };
    Ok(Status { exists, running })
}

pub fn print(status: &Status, name: &str) {
    let state = match (status.exists, status.running) {
        (false, _) => "absent",
        (true, false) => "stopped",
        (true, true) => "running",
    };
    println!("container: {} ({})", name, state);
}
```

- [ ] **Step 5: Build clean**

Run: `cargo build`
Expected: builds.

- [ ] **Step 6: Commit**

```bash
git add src/container/
git commit -m "feat(container): create/start/stop/down/exec/status primitives"
```

### Task 1.7: CLI definition and wiring

**Files:**
- Modify: `src/cli.rs`, `src/main.rs`

- [ ] **Step 1: Define clap structure**

`src/cli.rs`:

```rust
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
    /// Print or write defaults for global config / Dockerfile
    Init {
        #[arg(long)]
        force: bool,
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
}

#[derive(Subcommand, Debug)]
pub enum CsWorktreeCmd {
    Add { name: String, #[arg(long)] branch: Option<String> },
    Ls,
    Rm { name: String },
    Current,
}
```

- [ ] **Step 2: Wire `main.rs` to dispatch**

`src/main.rs`:

```rust
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use claude_sandbox::cli::{CsCli, CsCmd, Cmd, HostCli};
use claude_sandbox::container::{create::{ensure_container, CreateOptions}, exec::exec_into, lifecycle, status as st};
use claude_sandbox::error::Result;
use claude_sandbox::paths;
use claude_sandbox::podman::runner::Podman;
use claude_sandbox::project::{derive_name, find_project_root};
use claude_sandbox::{info, logging};

const DEFAULT_IMAGE: &str = "claude-sandbox:0.1";

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

    let cwd = std::env::current_dir()?;
    let project = find_project_root(&cwd)?;
    let name = derive_name(&project, &paths::home());
    let podman = Podman::discover()?;

    match cli.command.unwrap_or(Cmd::Start) {
        Cmd::Start => start_or_shell(&podman, &project, &name, "claude"),
        Cmd::Shell => start_or_shell(&podman, &project, &name, "bash"),
        Cmd::Stop => lifecycle::stop(&podman, &name),
        Cmd::Down => lifecycle::down(&podman, &name),
        Cmd::Status => {
            let s = st::collect(&podman, &name)?;
            st::print(&s, &name);
            Ok(())
        }
        Cmd::Ls { .. } | Cmd::Rebuild { .. } | Cmd::Logs | Cmd::Rename { .. }
        | Cmd::Migrate { .. } | Cmd::Worktree { .. } | Cmd::Init { .. } => {
            info!("command not yet implemented in this phase");
            Ok(())
        }
    }
}

fn start_or_shell(podman: &Podman, project: &std::path::Path, name: &str, inner: &str) -> Result<()> {
    ensure_container(
        podman,
        &CreateOptions {
            name,
            image: DEFAULT_IMAGE,
            project_path: project,
            ssh_agent: true,
        },
    )?;
    lifecycle::ensure_running(podman, name)?;
    exec_into(name, &[inner])
}

fn run_cs() -> Result<()> {
    let cli = CsCli::parse();
    logging::set_verbosity(cli.verbose);
    match cli.command {
        CsCmd::Status => {
            info!("cs status not yet implemented");
            Ok(())
        }
        CsCmd::Worktree { .. } => {
            info!("cs worktree not yet implemented");
            Ok(())
        }
    }
}
```

- [ ] **Step 3: Build clean and run --help**

Run: `cargo build && ./target/debug/claude-sandbox --help`
Expected: clap-formatted help listing all subcommands.

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat(cli): clap definitions and main dispatch (host + cs modes)"
```

### Task 1.8: CLI smoke test

**Files:**
- Test: `tests/cli_smoke.rs`

- [ ] **Step 1: Write the test**

```rust
use assert_cmd::Command;
use predicates::str::contains;

#[test]
fn help_works() {
    Command::cargo_bin("claude-sandbox").unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("start"))
        .stdout(contains("shell"))
        .stdout(contains("stop"))
        .stdout(contains("down"))
        .stdout(contains("rename"));
}
```

- [ ] **Step 2: Run test**

Run: `cargo test --test cli_smoke`
Expected: pass.

- [ ] **Step 3: Commit**

```bash
git add tests/cli_smoke.rs
git commit -m "test: cli help smoke"
```

### Task 1.9: Dockerfile asset + build/install scaffolding

**Files:**
- Create: `assets/Dockerfile`, `assets/default-config.toml`, `Makefile`

- [ ] **Step 1: Write the Dockerfile**

`assets/Dockerfile`:

```dockerfile
FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git sudo bash openssh-client \
      build-essential pkg-config jq direnv \
    && rm -rf /var/lib/apt/lists/*

# Tailscale (from official repo)
RUN curl -fsSL https://pkgs.tailscale.com/stable/debian/bookworm.noarmor.gpg \
      -o /usr/share/keyrings/tailscale-archive-keyring.gpg \
 && curl -fsSL https://pkgs.tailscale.com/stable/debian/bookworm.tailscale-keyring.list \
      -o /etc/apt/sources.list.d/tailscale.list \
 && apt-get update && apt-get install -y --no-install-recommends tailscale \
 && rm -rf /var/lib/apt/lists/*

# Claude Code (uses Anthropic's installer)
RUN curl -fsSL https://claude.ai/install.sh | bash || true

# Our binary; supplied at image build time alongside the Dockerfile.
COPY claude-sandbox /usr/local/bin/claude-sandbox
RUN chmod +x /usr/local/bin/claude-sandbox \
 && ln -sf claude-sandbox /usr/local/bin/cs

WORKDIR /work
ENTRYPOINT ["/bin/bash", "-l"]
```

`assets/default-config.toml`:

```toml
# Global defaults for claude-sandbox.
# Per-project .claude-sandbox.toml overrides these.

ssh_agent = true
network = "bridge"

# Example: globally available tools.
# setup = ["apt-get install -y ripgrep fd-find"]
```

- [ ] **Step 2: Write the Makefile**

`Makefile`:

```make
PREFIX ?= $(HOME)/.local
CONFIG_DIR := $(HOME)/.config/claude-sandbox

.PHONY: build install image clean

build:
	cargo build --release

install: build
	install -Dm755 target/release/claude-sandbox $(PREFIX)/bin/claude-sandbox
	install -d $(CONFIG_DIR)
	[ -f $(CONFIG_DIR)/Dockerfile ] || install -m644 assets/Dockerfile $(CONFIG_DIR)/Dockerfile
	[ -f $(CONFIG_DIR)/config.toml ] || install -m644 assets/default-config.toml $(CONFIG_DIR)/config.toml
	@echo "installed to $(PREFIX)/bin/claude-sandbox"

image:
	cp target/release/claude-sandbox $(CONFIG_DIR)/claude-sandbox
	podman build -t claude-sandbox:0.1 -f $(CONFIG_DIR)/Dockerfile $(CONFIG_DIR)
	rm $(CONFIG_DIR)/claude-sandbox

clean:
	cargo clean
```

- [ ] **Step 3: Verify install path**

Run: `make install`
Expected: binary at `~/.local/bin/claude-sandbox`, `Dockerfile` and `config.toml` at `~/.config/claude-sandbox/`. (Idempotent: re-running does not overwrite the latter two.)

- [ ] **Step 4: Verify image build**

Run: `make image` (requires podman, network).
Expected: image `claude-sandbox:0.1` exists; `podman images | grep claude-sandbox` shows it.

- [ ] **Step 5: Commit**

```bash
git add assets/ Makefile
git commit -m "build: Dockerfile, default config, Makefile (install + image)"
```

### Task 1.10: End-to-end smoke (real podman)

**Files:**
- Test: `tests/integration_podman.rs`

- [ ] **Step 1: Write a gated end-to-end test**

```rust
//! Real-podman tests. Skipped unless CLAUDE_SANDBOX_E2E=1.

use std::process::Command;

fn e2e_enabled() -> bool {
    std::env::var("CLAUDE_SANDBOX_E2E").ok().as_deref() == Some("1")
}

#[test]
fn e2e_lifecycle() {
    if !e2e_enabled() {
        eprintln!("skipping (set CLAUDE_SANDBOX_E2E=1 to run)");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join(".git")).unwrap();

    let bin = env!("CARGO_BIN_EXE_claude-sandbox");
    let run = |args: &[&str]| -> std::process::Output {
        Command::new(bin)
            .args(args)
            .current_dir(tmp.path())
            .output()
            .unwrap()
    };

    let s = run(&["status"]);
    assert!(s.status.success(), "status before create should not fail");

    // Start would block; instead create+start manually via stop/down to avoid attach.
    let stop = run(&["stop"]);
    // stop is allowed to fail when no container exists; tolerated.
    let _ = stop;
    let down = run(&["down"]);
    let _ = down;
}
```

- [ ] **Step 2: Run it (locally, with podman)**

Run: `CLAUDE_SANDBOX_E2E=1 cargo test --test integration_podman -- --nocapture`
Expected: at minimum `status` passes; lifecycle calls do not crash.

- [ ] **Step 3: Run a manual end-to-end check**

```bash
cd /tmp && mkdir e2e && cd e2e && git init -q
~/.local/bin/claude-sandbox status
~/.local/bin/claude-sandbox shell    # should drop into a bash inside cs-tmp-e2e
# inside the container:
#   ls /work
#   whoami            -> root
#   id                -> uid=0 mapped via userns
#   exit
~/.local/bin/claude-sandbox stop
~/.local/bin/claude-sandbox down
```

Expected:
- `shell` puts you in a bash where `/work` is the tmp dir;
- `apt-get install -y something` inside works;
- `stop` and `down` clean up.

- [ ] **Step 4: Commit**

```bash
git add tests/integration_podman.rs
git commit -m "test: opt-in e2e podman lifecycle smoke"
```

---

## Phase 2 — Config (`.claude-sandbox.toml`)

Adds load, validate, merge, auto-create, and plumbing into create.

### Task 2.1: Config schema (TDD)

**Files:**
- Modify: `src/config/mod.rs`, `src/config/parse.rs`
- Test: `tests/config_parse.rs`

- [ ] **Step 1: Define the schema**

`src/config/mod.rs`:

```rust
use std::collections::BTreeMap;

use serde::Deserialize;

pub mod edit;
pub mod parse;

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    pub name: Option<String>,
    #[serde(default)]
    pub agent_writable: bool,
    pub image: Option<String>,

    #[serde(default)]
    pub mount: Vec<MountSpec>,

    #[serde(default)]
    pub env_passthrough: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    pub env_file: Option<String>,

    pub ssh_agent: Option<bool>,
    pub network: Option<String>,
    #[serde(default)]
    pub ports: Vec<String>,

    #[serde(default)]
    pub tailscale: TailscaleSpec,

    #[serde(default)]
    pub gpu: bool,

    #[serde(default)]
    pub setup: Vec<String>,
    #[serde(default)]
    pub on_start: Vec<String>,
    #[serde(default)]
    pub on_stop: Vec<String>,
    #[serde(default)]
    pub worktree_setup: Vec<String>,

    #[serde(default)]
    pub limits: LimitsSpec,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MountSpec {
    pub host: String,
    pub container: String,
    #[serde(default)]
    pub ro: bool,
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TailscaleSpec {
    #[serde(default)]
    pub enabled: bool,
    pub hostname: Option<String>,
    #[serde(default = "default_authkey_env")]
    pub authkey_env: String,
}

fn default_authkey_env() -> String {
    "TS_AUTHKEY".into()
}

#[derive(Debug, Default, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LimitsSpec {
    pub memory: Option<String>,
    pub cpus: Option<f32>,
}
```

- [ ] **Step 2: Implement parse + validate**

`src/config/parse.rs`:

```rust
use std::path::Path;

use crate::error::{Error, Result};

use super::ConfigFile;

pub fn load(path: &Path) -> Result<ConfigFile> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| Error::Config(format!("reading {}: {e}", path.display())))?;
    let cfg: ConfigFile = toml::from_str(&raw)
        .map_err(|e| Error::Config(format!("parsing {}: {e}", path.display())))?;
    validate(&cfg, path)?;
    Ok(cfg)
}

pub fn load_optional(path: &Path) -> Result<Option<ConfigFile>> {
    if !path.exists() {
        return Ok(None);
    }
    load(path).map(Some)
}

pub fn validate(cfg: &ConfigFile, path: &Path) -> Result<()> {
    for m in &cfg.mount {
        if !std::path::Path::new(&m.container).is_absolute() {
            return Err(Error::Config(format!(
                "{}: mount.container '{}' must be absolute",
                path.display(),
                m.container
            )));
        }
    }
    if let Some(n) = &cfg.network {
        if !matches!(n.as_str(), "bridge" | "host" | "none") {
            return Err(Error::Config(format!(
                "{}: network '{}' must be one of: bridge, host, none",
                path.display(),
                n
            )));
        }
    }
    for p in &cfg.ports {
        let body = p.strip_prefix('!').unwrap_or(p);
        let (lhs, rhs) = body
            .split_once(':')
            .ok_or_else(|| Error::Config(format!("{}: bad port spec '{}'", path.display(), p)))?;
        if !lhs.is_empty() {
            lhs.parse::<u16>().map_err(|_| {
                Error::Config(format!("{}: bad host port in '{}'", path.display(), p))
            })?;
        }
        rhs.parse::<u16>().map_err(|_| {
            Error::Config(format!("{}: bad container port in '{}'", path.display(), p))
        })?;
    }
    Ok(())
}
```

- [ ] **Step 3: Write parse tests**

`tests/config_parse.rs`:

```rust
use std::fs;

use tempfile::tempdir;

use claude_sandbox::config::parse::load;

fn write(content: &str) -> tempfile::TempDir {
    let tmp = tempdir().unwrap();
    fs::write(tmp.path().join("c.toml"), content).unwrap();
    tmp
}

#[test]
fn parses_minimal() {
    let tmp = write("name = \"x\"\n");
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert_eq!(c.name.as_deref(), Some("x"));
    assert!(!c.agent_writable);
    assert!(c.mount.is_empty());
    assert_eq!(c.tailscale.authkey_env, "TS_AUTHKEY");
}

#[test]
fn parses_full() {
    let tmp = write(r#"
name = "p"
agent_writable = true
image = "claude-sandbox:0.1"
mount = [
  { host = "~/.config/pulumi", container = "/root/.config/pulumi", ro = true },
]
env_passthrough = ["TS_AUTHKEY"]
env = { CARGO_TERM_COLOR = "always" }
env_file = ".env"
ssh_agent = false
network = "bridge"
ports = ["5173:5173", "!8080:8080", ":3000"]

[tailscale]
enabled = true
hostname = "h"

gpu = true
setup = ["apt-get install -y x"]
worktree_setup = ["echo 1"]

[limits]
memory = "16g"
cpus = 4
"#);
    let c = load(&tmp.path().join("c.toml")).unwrap();
    assert!(c.agent_writable);
    assert_eq!(c.mount.len(), 1);
    assert_eq!(c.mount[0].host, "~/.config/pulumi");
    assert_eq!(c.tailscale.enabled, true);
    assert!(c.gpu);
    assert_eq!(c.limits.memory.as_deref(), Some("16g"));
    assert_eq!(c.limits.cpus, Some(4.0));
}

#[test]
fn rejects_unknown_field() {
    let tmp = write("name = \"x\"\nunknown_field = 1\n");
    assert!(load(&tmp.path().join("c.toml")).is_err());
}

#[test]
fn rejects_relative_mount_target() {
    let tmp = write(r#"mount = [{ host = "/x", container = "relative" }]"#);
    let e = load(&tmp.path().join("c.toml")).unwrap_err();
    assert!(format!("{e}").contains("must be absolute"));
}

#[test]
fn rejects_bad_port() {
    let tmp = write("ports = [\"hello:world\"]\n");
    assert!(load(&tmp.path().join("c.toml")).is_err());
}

#[test]
fn rejects_bad_network() {
    let tmp = write("network = \"weird\"\n");
    assert!(load(&tmp.path().join("c.toml")).is_err());
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --test config_parse`
Expected: 6 tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/config/mod.rs src/config/parse.rs tests/config_parse.rs
git commit -m "feat(config): toml schema + serde + validation"
```

### Task 2.2: Global + local merge

**Files:**
- Modify: `src/config/mod.rs`

- [ ] **Step 1: Add merge logic**

Append to `src/config/mod.rs`:

```rust
impl ConfigFile {
    /// Merge `other` *into* `self`: `other`'s fields override `self`'s.
    /// List-typed fields are concatenated (`self` first, then `other`).
    pub fn merge_in(&mut self, other: ConfigFile) {
        if other.name.is_some() {
            self.name = other.name;
        }
        if other.agent_writable {
            self.agent_writable = true;
        }
        if other.image.is_some() {
            self.image = other.image;
        }
        self.mount.extend(other.mount);
        self.env_passthrough.extend(other.env_passthrough);
        for (k, v) in other.env {
            self.env.insert(k, v);
        }
        if other.env_file.is_some() {
            self.env_file = other.env_file;
        }
        if other.ssh_agent.is_some() {
            self.ssh_agent = other.ssh_agent;
        }
        if other.network.is_some() {
            self.network = other.network;
        }
        self.ports.extend(other.ports);
        if other.tailscale.enabled {
            self.tailscale = other.tailscale;
        }
        if other.gpu {
            self.gpu = true;
        }
        self.setup.extend(other.setup);
        self.on_start.extend(other.on_start);
        self.on_stop.extend(other.on_stop);
        self.worktree_setup.extend(other.worktree_setup);
        if other.limits.memory.is_some() {
            self.limits.memory = other.limits.memory;
        }
        if other.limits.cpus.is_some() {
            self.limits.cpus = other.limits.cpus;
        }
    }
}

pub fn load_merged(global: Option<&std::path::Path>, local: Option<&std::path::Path>) -> crate::error::Result<ConfigFile> {
    let mut cfg = ConfigFile::default();
    if let Some(p) = global {
        if let Some(g) = parse::load_optional(p)? {
            cfg.merge_in(g);
        }
    }
    if let Some(p) = local {
        if let Some(l) = parse::load_optional(p)? {
            cfg.merge_in(l);
        }
    }
    Ok(cfg)
}
```

- [ ] **Step 2: Write a merge test**

Append to `tests/config_parse.rs`:

```rust
use claude_sandbox::config::ConfigFile;

#[test]
fn merge_overrides_scalars_and_concats_lists() {
    let mut a = ConfigFile::default();
    a.name = Some("a".into());
    a.setup = vec!["one".into()];

    let mut b = ConfigFile::default();
    b.name = Some("b".into());
    b.setup = vec!["two".into()];

    a.merge_in(b);
    assert_eq!(a.name.as_deref(), Some("b"));
    assert_eq!(a.setup, vec!["one".to_string(), "two".into()]);
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test --test config_parse`
Expected: all pass.

- [ ] **Step 4: Commit**

```bash
git add src/config/mod.rs tests/config_parse.rs
git commit -m "feat(config): global+local merge"
```

### Task 2.3: Auto-create `.claude-sandbox.toml` on first start

**Files:**
- Modify: `src/config/edit.rs`
- Test: `tests/config_auto_create.rs`

- [ ] **Step 1: Implement creator**

`src/config/edit.rs`:

```rust
use std::path::Path;

use toml_edit::{DocumentMut, value};

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
    doc["name"] = value(new_name);
    std::fs::write(path, doc.to_string())
        .map_err(|e| Error::Config(format!("writing {}: {e}", path.display())))?;
    Ok(())
}
```

- [ ] **Step 2: Test it**

`tests/config_auto_create.rs`:

```rust
use std::fs;

use tempfile::tempdir;

use claude_sandbox::config::edit::{create_minimal, set_name};

#[test]
fn creates_with_name_and_header() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join(".claude-sandbox.toml");
    create_minimal(&p, "documents-projects-spire").unwrap();
    let body = fs::read_to_string(&p).unwrap();
    assert!(body.starts_with("# claude-sandbox config"));
    assert!(body.contains("name = \"documents-projects-spire\""));
}

#[test]
fn create_minimal_is_idempotent() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join(".claude-sandbox.toml");
    create_minimal(&p, "a").unwrap();
    fs::write(&p, "# already custom\nname = \"a\"\n").unwrap();
    create_minimal(&p, "different").unwrap();
    let body = fs::read_to_string(&p).unwrap();
    assert!(body.contains("# already custom"));
    assert!(body.contains("\"a\""));
}

#[test]
fn rename_preserves_comments() {
    let tmp = tempdir().unwrap();
    let p = tmp.path().join("c.toml");
    fs::write(&p, "# header\n\nname = \"old\" # inline\n").unwrap();
    set_name(&p, "new").unwrap();
    let body = fs::read_to_string(&p).unwrap();
    assert!(body.contains("# header"));
    assert!(body.contains("# inline"));
    assert!(body.contains("\"new\""));
}
```

- [ ] **Step 3: Run**

Run: `cargo test --test config_auto_create`
Expected: 3 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/config/edit.rs tests/config_auto_create.rs
git commit -m "feat(config): auto-create + name set via toml_edit (preserves comments)"
```

### Task 2.4: Plumb config through `start`

**Files:**
- Modify: `src/main.rs`, `src/container/create.rs`, `src/mounts.rs`, `src/env.rs`

- [ ] **Step 1: Build env vec from config**

`src/env.rs`:

```rust
use std::collections::BTreeMap;

use crate::config::ConfigFile;
use crate::paths;

pub fn resolve(cfg: &ConfigFile, project: &std::path::Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for (k, v) in &cfg.env {
        out.push((k.clone(), paths::expand(v)));
    }
    for k in &cfg.env_passthrough {
        if let Ok(v) = std::env::var(k) {
            out.push((k.clone(), v));
        }
    }
    if let Some(f) = &cfg.env_file {
        let p = project.join(f);
        if let Ok(s) = std::fs::read_to_string(&p) {
            for line in s.lines() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                if let Some((k, v)) = line.split_once('=') {
                    out.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
        }
    }
    out
}

pub fn ensure_ssh_agent(env: &mut Vec<(String, String)>, volumes: &mut Vec<crate::mounts::Volume>) {
    if let Some(sock) = crate::network::ssh_agent_socket() {
        volumes.push(crate::mounts::Volume::Bind(crate::mounts::Mount {
            host: sock,
            container: std::path::PathBuf::from("/ssh-agent.sock"),
            ro: false,
        }));
        env.push(("SSH_AUTH_SOCK".into(), "/ssh-agent.sock".into()));
    }
}
```

- [ ] **Step 2: Add config mounts and the toml read-only mount**

Append to `src/mounts.rs`:

```rust
use crate::config::{ConfigFile, MountSpec};
use crate::paths;

pub fn extra_volumes(cfg: &ConfigFile, project: &Path) -> Vec<Volume> {
    cfg.mount
        .iter()
        .map(|m| spec_to_volume(m, project))
        .collect()
}

fn spec_to_volume(m: &MountSpec, project: &Path) -> Volume {
    let host = if m.host.starts_with('/') || m.host.starts_with('~') || m.host.starts_with('$') {
        std::path::PathBuf::from(paths::expand(&m.host))
    } else {
        project.join(&m.host)
    };
    Volume::Bind(Mount {
        host,
        container: PathBuf::from(&m.container),
        ro: m.ro,
    })
}

pub fn toml_mount(project: &Path, agent_writable: bool) -> Volume {
    Volume::Bind(Mount {
        host: project.join(".claude-sandbox.toml"),
        container: PathBuf::from("/work/.claude-sandbox.toml"),
        ro: !agent_writable,
    })
}

pub fn assert_no_target_collisions(volumes: &[Volume]) -> crate::error::Result<()> {
    use std::collections::HashMap;
    let mut seen: HashMap<&Path, ()> = HashMap::new();
    for v in volumes {
        let target = match v {
            Volume::Bind(m) => m.container.as_path(),
            Volume::Named { container, .. } => container.as_path(),
        };
        if seen.insert(target, ()).is_some() {
            return Err(crate::error::Error::Config(format!(
                "mount collision at {}",
                target.display()
            )));
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Update `create.rs` to use config**

Replace `src/container/create.rs`:

```rust
use std::path::Path;

use crate::config::ConfigFile;
use crate::env;
use crate::error::Result;
use crate::mounts::{
    assert_no_target_collisions, default_volumes, extra_volumes, toml_mount,
};
use crate::podman::args::{create_args, CreateSpec};
use crate::podman::runner::Podman;

pub struct CreateOptions<'a> {
    pub name: &'a str,
    pub image: &'a str,
    pub project_path: &'a Path,
    pub config: &'a ConfigFile,
}

pub fn ensure_container(podman: &Podman, opts: &CreateOptions) -> Result<()> {
    if podman.container_exists(opts.name)? {
        return Ok(());
    }
    let mut volumes = default_volumes(opts.project_path, opts.name);
    volumes.extend(extra_volumes(opts.config, opts.project_path));
    if opts
        .project_path
        .join(".claude-sandbox.toml")
        .exists()
    {
        volumes.push(toml_mount(opts.project_path, opts.config.agent_writable));
    }

    let mut env_pairs = env::resolve(opts.config, opts.project_path);
    if opts.config.ssh_agent.unwrap_or(true) {
        env::ensure_ssh_agent(&mut env_pairs, &mut volumes);
    }

    assert_no_target_collisions(&volumes)?;

    let network = opts.config.network.as_deref().unwrap_or("bridge");
    let workdir = std::path::PathBuf::from("/work");
    let spec = CreateSpec {
        name: opts.name,
        image: opts.image,
        volumes: &volumes,
        env: &env_pairs,
        network,
        ports: &[],
        workdir: &workdir,
        extra: &[],
    };
    podman.run(&create_args(&spec))?;
    Ok(())
}
```

- [ ] **Step 4: Update `main.rs` start path**

Replace the `start_or_shell` and the dispatch of `Cmd::Start`/`Cmd::Shell` in `src/main.rs` to load config and auto-create the toml on first run:

```rust
fn start_or_shell(podman: &Podman, project: &std::path::Path, derived_name: &str, inner: &str) -> Result<()> {
    use claude_sandbox::config::{edit, load_merged};
    let toml_path = project.join(".claude-sandbox.toml");

    if !toml_path.exists() {
        edit::create_minimal(&toml_path, derived_name)?;
    }

    let global = paths::config_dir().join("config.toml");
    let cfg = load_merged(Some(&global), Some(&toml_path))?;
    let name = cfg.name.clone().unwrap_or_else(|| derived_name.to_string());
    let image = cfg.image.clone().unwrap_or_else(|| DEFAULT_IMAGE.into());

    use claude_sandbox::container::create::{ensure_container, CreateOptions};
    ensure_container(
        podman,
        &CreateOptions {
            name: &name,
            image: &image,
            project_path: project,
            config: &cfg,
        },
    )?;
    lifecycle::ensure_running(podman, &name)?;
    exec_into(&name, &[inner])
}
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add src/main.rs src/container/create.rs src/env.rs src/mounts.rs
git commit -m "feat(start): load config, auto-create toml, mount config + extras"
```

### Task 2.5: Hooks executor

**Files:**
- Modify: `src/hooks.rs`, `src/container/create.rs`, `src/container/lifecycle.rs`

- [ ] **Step 1: Implement the executor**

`src/hooks.rs`:

```rust
use std::collections::BTreeMap;

use crate::error::{Error, Result};
use crate::podman::runner::Podman;

pub struct HookEnv {
    pub project_name: String,
    pub project_path: std::path::PathBuf,
    pub worktree_name: Option<String>,
}

pub fn run(
    podman: &Podman,
    container: &str,
    commands: &[String],
    env: &HookEnv,
    abort_on_failure: bool,
) -> Result<()> {
    if commands.is_empty() {
        return Ok(());
    }
    let mut env_pairs: BTreeMap<&str, String> = BTreeMap::new();
    env_pairs.insert("CS_PROJECT_NAME", env.project_name.clone());
    env_pairs.insert(
        "CS_PROJECT_PATH",
        env.project_path.display().to_string(),
    );
    if let Some(w) = &env.worktree_name {
        env_pairs.insert("CS_WORKTREE_NAME", w.clone());
    }

    let script = commands.join(" && ");
    let mut args: Vec<String> = vec!["exec".into()];
    for (k, v) in &env_pairs {
        args.push("--env".into());
        args.push(format!("{}={}", k, v));
    }
    args.push(container.into());
    args.push("bash".into());
    args.push("-c".into());
    args.push(script);

    match podman.run(&args) {
        Ok(_) => Ok(()),
        Err(e) => {
            if abort_on_failure {
                Err(e)
            } else {
                eprintln!("[warn] hook failed (continuing): {e}");
                Ok(())
            }
        }
    }
}
```

- [ ] **Step 2: Wire `setup` into create after first start, and `on_start` into start**

Update `src/container/create.rs` — add a `run_setup` function called after first create succeeds:

```rust
pub fn run_setup(
    podman: &Podman,
    name: &str,
    project_path: &Path,
    setup: &[String],
) -> Result<()> {
    if setup.is_empty() {
        return Ok(());
    }
    // Container must be running for exec.
    podman.run(&crate::podman::args::start_args(name))?;
    crate::hooks::run(
        podman,
        name,
        setup,
        &crate::hooks::HookEnv {
            project_name: name.to_string(),
            project_path: project_path.to_path_buf(),
            worktree_name: None,
        },
        true,
    )?;
    Ok(())
}
```

Update `start_or_shell` in `main.rs` to:
1. Detect if container was just created this call (return value from `ensure_container`).
2. If so, run `setup` hooks.
3. Always run `on_start` hooks after `ensure_running`.

Change `ensure_container` to return `bool` for "newly created":

```rust
pub fn ensure_container(podman: &Podman, opts: &CreateOptions) -> Result<bool> {
    if podman.container_exists(opts.name)? {
        return Ok(false);
    }
    // ...existing body...
    podman.run(&create_args(&spec))?;
    Ok(true)
}
```

In `main.rs`:

```rust
let just_created = ensure_container(podman, &CreateOptions { ... })?;
if just_created {
    create::run_setup(podman, &name, project, &cfg.setup)?;
}
lifecycle::ensure_running(podman, &name)?;
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
)?;
exec_into(&name, &[inner])
```

(Add the `claude_sandbox::container::create` import, and `claude_sandbox::hooks`.)

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/hooks.rs src/container/create.rs src/main.rs
git commit -m "feat(hooks): setup (abort-on-fail) and on_start (warn-on-fail) hooks"
```

### Task 2.6: Stop runs `on_stop`

**Files:**
- Modify: `src/container/lifecycle.rs`, `src/main.rs`

- [ ] **Step 1: Update stop to take hooks**

```rust
pub fn stop(podman: &Podman, name: &str, on_stop: &[String], project: &std::path::Path) -> Result<()> {
    if !on_stop.is_empty() && podman.container_running(name)? {
        crate::hooks::run(
            podman,
            name,
            on_stop,
            &crate::hooks::HookEnv {
                project_name: name.to_string(),
                project_path: project.to_path_buf(),
                worktree_name: None,
            },
            false,
        )?;
    }
    podman.run(&stop_args(name))?;
    Ok(())
}
```

- [ ] **Step 2: Update `main.rs` to load config and pass it to stop**

```rust
Cmd::Stop => {
    let cfg = load_cfg(&project)?;
    lifecycle::stop(&podman, &name, &cfg.on_stop, &project)
}
```

Where `load_cfg` factors out the global+local load.

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/container/lifecycle.rs src/main.rs
git commit -m "feat(stop): run on_stop hooks before podman stop"
```

---

## Phase 3 — `cs` companion + worktrees

Adds worktree management as both an in-container `cs` command and a host-side `claude-sandbox worktree` command.

### Task 3.1: Worktree primitives (TDD)

**Files:**
- Modify: `src/worktree/mod.rs`, `src/worktree/commands.rs`
- Test: `tests/worktree.rs`

- [ ] **Step 1: Define types**

`src/worktree/mod.rs`:

```rust
pub mod claim;
pub mod commands;

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub name: String,
    pub path: PathBuf,
    pub branch: Option<String>,
}
```

- [ ] **Step 2: Implement list parsing from `git worktree list --porcelain`**

`src/worktree/commands.rs`:

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

use super::WorktreeInfo;

pub fn list(project: &Path) -> Result<Vec<WorktreeInfo>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["worktree", "list", "--porcelain"])
        .output()?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "git worktree list failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(parse_porcelain(
        std::str::from_utf8(&out.stdout).unwrap_or(""),
        project,
    ))
}

pub fn parse_porcelain(text: &str, project: &Path) -> Vec<WorktreeInfo> {
    let mut out: Vec<WorktreeInfo> = Vec::new();
    let mut path: Option<PathBuf> = None;
    let mut branch: Option<String> = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            if let Some(p) = path.take() {
                out.push(WorktreeInfo {
                    name: classify(&p, project),
                    path: p,
                    branch: branch.take(),
                });
            }
            path = Some(PathBuf::from(rest));
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = Some(rest.trim_start_matches("refs/heads/").to_string());
        }
    }
    if let Some(p) = path.take() {
        out.push(WorktreeInfo {
            name: classify(&p, project),
            path: p,
            branch,
        });
    }
    out
}

fn classify(path: &Path, project: &Path) -> String {
    if path == project {
        "main".to_string()
    } else if let Ok(rel) = path.strip_prefix(project.join(".worktrees")) {
        rel.to_string_lossy().to_string()
    } else {
        path.file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "?".to_string())
    }
}
```

- [ ] **Step 3: Write test**

`tests/worktree.rs`:

```rust
use std::path::PathBuf;

use claude_sandbox::worktree::commands::parse_porcelain;

#[test]
fn parses_main_and_worktree() {
    let project = PathBuf::from("/work");
    let text = "worktree /work\nHEAD abcd\nbranch refs/heads/main\n\nworktree /work/.worktrees/feat-x\nHEAD efgh\nbranch refs/heads/feat-x\n";
    let v = parse_porcelain(text, &project);
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].name, "main");
    assert_eq!(v[0].branch.as_deref(), Some("main"));
    assert_eq!(v[1].name, "feat-x");
    assert_eq!(v[1].path, PathBuf::from("/work/.worktrees/feat-x"));
    assert_eq!(v[1].branch.as_deref(), Some("feat-x"));
}
```

- [ ] **Step 4: Run**

Run: `cargo test --test worktree`
Expected: 1 test passes.

- [ ] **Step 5: Commit**

```bash
git add src/worktree/mod.rs src/worktree/commands.rs tests/worktree.rs
git commit -m "feat(worktree): list/parse git worktrees"
```

### Task 3.2: `cs worktree add` (inside container)

**Files:**
- Modify: `src/worktree/commands.rs`, `src/main.rs`

- [ ] **Step 1: Implement add**

Append to `src/worktree/commands.rs`:

```rust
pub fn add(project: &Path, name: &str, branch: Option<&str>) -> Result<PathBuf> {
    let dir = project.join(".worktrees").join(name);
    if dir.exists() {
        return Err(Error::Other(format!("worktree {} already exists", name)));
    }
    std::fs::create_dir_all(project.join(".worktrees"))?;
    let mut args: Vec<String> = vec!["worktree".into(), "add".into()];
    if let Some(b) = branch {
        args.push(dir.display().to_string());
        args.push(b.to_string());
    } else {
        args.push("-b".into());
        args.push(name.to_string());
        args.push(dir.display().to_string());
    }
    let status = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(&args)
        .status()?;
    if !status.success() {
        return Err(Error::Other(format!("git worktree add failed for {name}")));
    }
    ensure_gitignore_entry(project)?;
    Ok(dir)
}

fn ensure_gitignore_entry(project: &Path) -> Result<()> {
    let p = project.join(".gitignore");
    let needle = ".worktrees/\n";
    let current = std::fs::read_to_string(&p).unwrap_or_default();
    if !current.contains(".worktrees/") {
        let mut s = current;
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(needle);
        std::fs::write(&p, s)?;
    }
    Ok(())
}

pub fn remove(project: &Path, name: &str) -> Result<()> {
    let dir = project.join(".worktrees").join(name);
    let status = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["worktree", "remove", "--force"])
        .arg(&dir)
        .status()?;
    if !status.success() {
        return Err(Error::Other(format!("git worktree remove failed for {name}")));
    }
    // git worktree prune is automatic post-remove but explicit doesn't hurt.
    let _ = Command::new("git")
        .arg("-C")
        .arg(project)
        .args(["worktree", "prune"])
        .status();
    Ok(())
}

pub fn current(cwd: &Path, project: &Path) -> String {
    if cwd == project {
        return "main".to_string();
    }
    if let Ok(rel) = cwd.strip_prefix(project.join(".worktrees")) {
        if let Some(first) = rel.components().next() {
            return first.as_os_str().to_string_lossy().to_string();
        }
    }
    "main".to_string()
}
```

- [ ] **Step 2: Wire `cs` subcommands in `run_cs()` in `main.rs`**

```rust
use claude_sandbox::cli::{CsCmd, CsWorktreeCmd};
use claude_sandbox::worktree::commands as wt;

fn run_cs() -> Result<()> {
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
```

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: clean build.

- [ ] **Step 4: Commit**

```bash
git add src/worktree/commands.rs src/main.rs
git commit -m "feat(cs): worktree add/ls/rm/current inside container, with hooks"
```

### Task 3.3: Host `claude-sandbox worktree ls` and `rm`

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Wire host worktree subcommands**

In the `Cmd::Worktree { cmd }` arm of `run_host`:

```rust
Cmd::Worktree { cmd } => match cmd {
    claude_sandbox::cli::WorktreeCmd::Ls => {
        // run cs worktree ls via podman exec into the live container
        ensure_running_if_exists(&podman, &name)?;
        podman.run_inherit(&["exec".into(), name.clone(), "cs".into(), "worktree".into(), "ls".into()])
    }
    claude_sandbox::cli::WorktreeCmd::Rm { name: wt_name } => {
        ensure_running_if_exists(&podman, &name)?;
        podman.run_inherit(&[
            "exec".into(),
            name.clone(),
            "cs".into(),
            "worktree".into(),
            "rm".into(),
            wt_name,
        ])
    }
},
```

Helper:

```rust
fn ensure_running_if_exists(podman: &Podman, name: &str) -> Result<()> {
    if !podman.container_exists(name)? {
        return Err(claude_sandbox::error::Error::Other(format!(
            "no container for this project; run `claude-sandbox start` first"
        )));
    }
    lifecycle::ensure_running(podman, name)
}
```

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(host): worktree ls/rm proxied to in-container cs"
```

---

## Phase 4 — Picker + claim files

### Task 4.1: Claim file lifecycle (TDD)

**Files:**
- Modify: `src/worktree/claim.rs`
- Test: `tests/claim.rs`

- [ ] **Step 1: Implement**

`src/worktree/claim.rs`:

```rust
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub host_pid: i32,
    pub started_at: u64,
    pub container_exec_id: Option<String>,
}

pub fn claim_path(worktree_dir: &Path) -> PathBuf {
    worktree_dir.join(".cs-session")
}

pub fn write(worktree_dir: &Path) -> Result<Claim> {
    let claim = Claim {
        host_pid: std::process::id() as i32,
        started_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        container_exec_id: None,
    };
    let body = serde_json::to_string_pretty(&claim).map_err(|e| Error::Other(e.to_string()))?;
    std::fs::write(claim_path(worktree_dir), body)?;
    Ok(claim)
}

pub fn read(worktree_dir: &Path) -> Result<Option<Claim>> {
    let p = claim_path(worktree_dir);
    if !p.exists() {
        return Ok(None);
    }
    let body = std::fs::read_to_string(&p)?;
    let claim: Claim = serde_json::from_str(&body).map_err(|e| Error::Other(e.to_string()))?;
    Ok(Some(claim))
}

pub fn clear(worktree_dir: &Path) -> Result<()> {
    let p = claim_path(worktree_dir);
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

pub fn pid_alive(pid: i32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    kill(Pid::from_raw(pid), None).is_ok()
}

pub enum ClaimState {
    Available,
    Active(Claim),
    Stale(Claim),
}

pub fn evaluate(worktree_dir: &Path) -> Result<ClaimState> {
    Ok(match read(worktree_dir)? {
        None => ClaimState::Available,
        Some(c) if pid_alive(c.host_pid) => ClaimState::Active(c),
        Some(c) => ClaimState::Stale(c),
    })
}
```

- [ ] **Step 2: Test**

`tests/claim.rs`:

```rust
use tempfile::tempdir;

use claude_sandbox::worktree::claim::{clear, evaluate, read, write, ClaimState};

#[test]
fn write_then_read_roundtrips() {
    let tmp = tempdir().unwrap();
    let c = write(tmp.path()).unwrap();
    let read_back = read(tmp.path()).unwrap().unwrap();
    assert_eq!(read_back.host_pid, c.host_pid);
}

#[test]
fn clear_removes_file() {
    let tmp = tempdir().unwrap();
    write(tmp.path()).unwrap();
    clear(tmp.path()).unwrap();
    assert!(read(tmp.path()).unwrap().is_none());
}

#[test]
fn active_when_pid_is_self() {
    let tmp = tempdir().unwrap();
    write(tmp.path()).unwrap();
    match evaluate(tmp.path()).unwrap() {
        ClaimState::Active(_) => {}
        _ => panic!("expected Active"),
    }
}

#[test]
fn stale_when_pid_does_not_exist() {
    use std::fs;
    let tmp = tempdir().unwrap();
    fs::write(
        tmp.path().join(".cs-session"),
        r#"{"host_pid":1,"started_at":0,"container_exec_id":null}"#,
    ).unwrap();
    // PID 1 *is* alive on most systems (init); pick a clearly-bogus high PID.
    fs::write(
        tmp.path().join(".cs-session"),
        r#"{"host_pid":2147483640,"started_at":0,"container_exec_id":null}"#,
    ).unwrap();
    match evaluate(tmp.path()).unwrap() {
        ClaimState::Stale(_) => {}
        _ => panic!("expected Stale"),
    }
}

#[test]
fn available_when_no_file() {
    let tmp = tempdir().unwrap();
    match evaluate(tmp.path()).unwrap() {
        ClaimState::Available => {}
        _ => panic!("expected Available"),
    }
}
```

Add `serde_json` to `[dev-dependencies]` if not already there (it isn't yet — add it).

Update `Cargo.toml` `[dependencies]` to include `serde_json` (it already does from Task 0.1).

- [ ] **Step 3: Run**

Run: `cargo test --test claim`
Expected: 5 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/worktree/claim.rs tests/claim.rs
git commit -m "feat(claim): .cs-session lifecycle with PID liveness"
```

### Task 4.2: Picker UI

**Files:**
- Modify: `src/picker.rs`

- [ ] **Step 1: Implement**

`src/picker.rs`:

```rust
use std::path::Path;

use dialoguer::Select;

use crate::error::{Error, Result};
use crate::worktree::claim::{evaluate, ClaimState};
use crate::worktree::commands::list as list_worktrees;
use crate::worktree::WorktreeInfo;

pub enum Choice {
    Main,
    Existing(String),
    New(String, Option<String>),
    Quit,
}

pub fn pick(project: &Path) -> Result<Choice> {
    let entries = build_entries(project)?;
    let labels: Vec<String> = entries.iter().map(label).collect();
    let mut labels_with_actions = labels.clone();
    labels_with_actions.push("+ new worktree".into());
    labels_with_actions.push("quit".into());

    let idx = Select::new()
        .with_prompt("Choose")
        .items(&labels_with_actions)
        .default(0)
        .interact()
        .map_err(|e| Error::Other(format!("picker: {e}")))?;

    if idx == labels_with_actions.len() - 1 {
        return Ok(Choice::Quit);
    }
    if idx == labels_with_actions.len() - 2 {
        let name: String = dialoguer::Input::new()
            .with_prompt("Worktree name")
            .interact_text()
            .map_err(|e| Error::Other(format!("input: {e}")))?;
        let branch: String = dialoguer::Input::new()
            .with_prompt("Branch (empty = new branch from HEAD)")
            .allow_empty(true)
            .interact_text()
            .map_err(|e| Error::Other(format!("input: {e}")))?;
        let branch = if branch.is_empty() { None } else { Some(branch) };
        return Ok(Choice::New(name, branch));
    }

    let entry = &entries[idx];
    Ok(if entry.name == "main" {
        Choice::Main
    } else {
        Choice::Existing(entry.name.clone())
    })
}

fn build_entries(project: &Path) -> Result<Vec<WorktreeInfo>> {
    list_worktrees(project)
}

fn label(w: &WorktreeInfo) -> String {
    let state = if w.name == "main" {
        "main".to_string()
    } else {
        match evaluate(&w.path).unwrap_or(ClaimState::Available) {
            ClaimState::Available => "available".into(),
            ClaimState::Active(c) => format!(
                "in-use: host PID {} since epoch {}",
                c.host_pid, c.started_at
            ),
            ClaimState::Stale(c) => format!("stale claim PID {} — will reclaim", c.host_pid),
        }
    };
    format!("{}  [{}]", w.name, state)
}

pub fn has_worktrees(project: &Path) -> bool {
    let p = project.join(".worktrees");
    p.is_dir() && std::fs::read_dir(&p).map(|r| r.count() > 0).unwrap_or(false)
}
```

- [ ] **Step 2: Wire into start**

In `run_host`'s `Cmd::Start` arm:

```rust
Cmd::Start => {
    if cli.main || cli.worktree.is_some() {
        let inner = "claude";
        return targeted_start(&podman, &project, &name, inner, cli.worktree.as_deref(), cli.force);
    }
    if !claude_sandbox::picker::has_worktrees(&project) {
        return start_or_shell(&podman, &project, &name, "claude");
    }
    if cli.no_menu {
        return Err(claude_sandbox::error::Error::Other(
            "menu would have shown but --no-menu was given".into(),
        ));
    }
    match claude_sandbox::picker::pick(&project)? {
        claude_sandbox::picker::Choice::Quit => Ok(()),
        claude_sandbox::picker::Choice::Main => start_or_shell(&podman, &project, &name, "claude"),
        claude_sandbox::picker::Choice::Existing(w) => {
            targeted_start(&podman, &project, &name, "claude", Some(&w), cli.force)
        }
        claude_sandbox::picker::Choice::New(w, b) => {
            create_worktree_and_start(&podman, &project, &name, &w, b.as_deref())
        }
    }
}
```

Add helpers:

```rust
fn targeted_start(
    podman: &Podman,
    project: &Path,
    container: &str,
    inner: &str,
    worktree: Option<&str>,
    force: bool,
) -> Result<()> {
    start_or_shell(podman, project, container, inner)?;
    // Once the wrapper has execvp'd, this is unreachable; left for clarity.
    let _ = (worktree, force);
    Ok(())
}

fn create_worktree_and_start(
    podman: &Podman,
    project: &Path,
    container: &str,
    worktree: &str,
    branch: Option<&str>,
) -> Result<()> {
    // Ensure container running first so cs is available.
    ensure_running_only(podman, container)?;
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
    claude_sandbox::container::exec::exec_into(
        container,
        &[
            "bash", "-lc",
            &format!("cd /work/.worktrees/{} && claude", worktree),
        ],
    )
}
```

Note: `targeted_start` and the worktree-aware `claude` invocation should also write a claim file. Defer that to the next task — keep this step focused on wiring the picker.

- [ ] **Step 3: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/picker.rs src/main.rs
git commit -m "feat(picker): worktree picker with status annotations"
```

### Task 4.3: Wire claim files into worktree launch

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Refactor worktree launch to write/clear claim**

Change `create_worktree_and_start` and add a `start_in_worktree`:

```rust
fn start_in_worktree(
    podman: &Podman,
    project: &Path,
    container: &str,
    worktree: &str,
    inner: &str,
    force: bool,
) -> Result<()> {
    use claude_sandbox::worktree::claim::{self, ClaimState};
    let wt_dir = project.join(".worktrees").join(worktree);
    ensure_running_only(podman, container)?;

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

fn targeted_start(
    podman: &Podman,
    project: &Path,
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
```

(`ensure_running_only` skips `ensure_container` since the container should already exist by this point.)

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 3: Manual verify**

```bash
cd ~/some-rust-project
~/.local/bin/claude-sandbox            # no worktrees, drops into claude in main
# in another terminal:
~/.local/bin/claude-sandbox shell
# inside: cs worktree add feat-a
# back on host:
~/.local/bin/claude-sandbox            # picker shows feat-a as available
~/.local/bin/claude-sandbox -w feat-a
# in a third terminal, run -w feat-a again → "in use by PID ..."
```

- [ ] **Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(claim): write/clear claim files around worktree launches"
```

---

## Phase 5 — Ports, Tailscale, GPU, rename, migrate

### Task 5.1: Port spec parsing + shift probe (TDD)

**Files:**
- Modify: `src/network.rs`
- Test: `tests/ports.rs`

- [ ] **Step 1: Implement**

Append to `src/network.rs`:

```rust
use std::net::TcpListener;

use crate::error::{Error, Result};
use crate::podman::args::PortMapping;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRequest {
    pub host: Option<u16>,
    pub container: u16,
    pub strict: bool,
}

pub fn parse(spec: &str) -> Result<PortRequest> {
    let (strict, body) = if let Some(rest) = spec.strip_prefix('!') {
        (true, rest)
    } else {
        (false, spec)
    };
    let (lhs, rhs) = body.split_once(':').ok_or_else(|| {
        Error::Config(format!("port spec '{spec}' missing colon"))
    })?;
    let host = if lhs.is_empty() {
        None
    } else {
        Some(lhs.parse().map_err(|_| Error::Config(format!("bad host port in '{spec}'")))?)
    };
    let container = rhs
        .parse()
        .map_err(|_| Error::Config(format!("bad container port in '{spec}'")))?;
    Ok(PortRequest {
        host,
        container,
        strict,
    })
}

pub fn resolve(reqs: &[PortRequest]) -> Result<Vec<PortMapping>> {
    let mut out = Vec::new();
    let mut taken = std::collections::HashSet::new();
    for r in reqs {
        let host = pick_host_port(r, &taken)?;
        taken.insert(host);
        out.push(PortMapping {
            host,
            container: r.container,
        });
    }
    Ok(out)
}

fn pick_host_port(r: &PortRequest, taken: &std::collections::HashSet<u16>) -> Result<u16> {
    match r.host {
        None => {
            // ephemeral
            let l = TcpListener::bind("127.0.0.1:0")
                .map_err(|e| Error::Other(format!("ephemeral port bind: {e}")))?;
            let p = l.local_addr().unwrap().port();
            drop(l);
            Ok(p)
        }
        Some(p) => {
            if r.strict {
                if !port_free(p) || taken.contains(&p) {
                    return Err(Error::Other(format!("port {p} unavailable (strict)")));
                }
                return Ok(p);
            }
            for delta in 0..=20u16 {
                let candidate = p.saturating_add(delta);
                if candidate == 0 {
                    break;
                }
                if !taken.contains(&candidate) && port_free(candidate) {
                    return Ok(candidate);
                }
            }
            // Fallback: ephemeral
            let l = TcpListener::bind("127.0.0.1:0")?;
            Ok(l.local_addr().unwrap().port())
        }
    }
}

fn port_free(p: u16) -> bool {
    TcpListener::bind(("127.0.0.1", p)).is_ok()
}
```

- [ ] **Step 2: Test**

`tests/ports.rs`:

```rust
use std::net::TcpListener;

use claude_sandbox::network::{parse, resolve, PortRequest};

#[test]
fn parses_preferred() {
    let r = parse("5173:5173").unwrap();
    assert_eq!(r, PortRequest { host: Some(5173), container: 5173, strict: false });
}

#[test]
fn parses_strict() {
    let r = parse("!8080:8080").unwrap();
    assert_eq!(r, PortRequest { host: Some(8080), container: 8080, strict: true });
}

#[test]
fn parses_ephemeral() {
    let r = parse(":3000").unwrap();
    assert_eq!(r, PortRequest { host: None, container: 3000, strict: false });
}

#[test]
fn rejects_missing_colon() {
    assert!(parse("hello").is_err());
}

#[test]
fn shift_picks_next_free() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = listener.local_addr().unwrap().port();
    let r = PortRequest { host: Some(taken), container: 9999, strict: false };
    let mapped = resolve(&[r]).unwrap();
    assert_eq!(mapped[0].container, 9999);
    assert_ne!(mapped[0].host, taken);
}

#[test]
fn strict_errors_when_unavailable() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let taken = listener.local_addr().unwrap().port();
    let r = PortRequest { host: Some(taken), container: 9999, strict: true };
    assert!(resolve(&[r]).is_err());
}
```

- [ ] **Step 3: Run**

Run: `cargo test --test ports`
Expected: 6 tests pass.

- [ ] **Step 4: Commit**

```bash
git add src/network.rs tests/ports.rs
git commit -m "feat(network): port spec parser + free-port shift probe"
```

### Task 5.2: Wire ports into create

**Files:**
- Modify: `src/container/create.rs`, `src/main.rs`

- [ ] **Step 1: Plumb ports through**

In `ensure_container`, parse `cfg.ports`, resolve, pass to `CreateSpec`:

```rust
let port_requests: Vec<crate::network::PortRequest> = opts
    .config
    .ports
    .iter()
    .map(|s| crate::network::parse(s))
    .collect::<Result<Vec<_>>>()?;
let ports = crate::network::resolve(&port_requests)?;

// pass &ports as spec.ports
```

After creation, if any port differs from preference, print the actual mapping.

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/container/create.rs src/main.rs
git commit -m "feat(ports): use parsed+resolved ports in container create"
```

### Task 5.3: Tailscale feature

**Files:**
- Modify: `src/features/tailscale.rs`, `src/container/create.rs`

- [ ] **Step 1: Add on-start tailscale start commands**

`src/features/tailscale.rs`:

```rust
use crate::config::TailscaleSpec;
use crate::hooks::HookEnv;
use crate::error::Result;
use crate::podman::runner::Podman;

pub fn on_start_commands(spec: &TailscaleSpec, container_name: &str) -> Vec<String> {
    if !spec.enabled {
        return Vec::new();
    }
    let hostname = spec.hostname.clone().unwrap_or_else(|| container_name.to_string());
    vec![
        "pidof tailscaled >/dev/null || \
         (tailscaled --tun=userspace-networking --statedir=/var/lib/tailscale > /var/log/tailscaled.log 2>&1 &)".into(),
        format!("tailscale up --authkey=\"${{{authkey}}}\" --hostname=\"{hostname}\" --accept-dns=false --accept-routes=false || true", authkey = spec.authkey_env),
    ]
}

pub fn passthrough_env(spec: &TailscaleSpec) -> Vec<String> {
    if spec.enabled {
        vec![spec.authkey_env.clone()]
    } else {
        vec![]
    }
}
```

In `start_or_shell` (or wherever `on_start` hooks run), prepend the tailscale start commands when enabled.

- [ ] **Step 2: Build**

Run: `cargo build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add src/features/tailscale.rs src/main.rs src/container/create.rs
git commit -m "feat(tailscale): per-project tailscaled + tailscale up via on_start"
```

### Task 5.4: GPU opt-in

**Files:**
- Modify: `src/features/gpu.rs`, `src/container/create.rs`

- [ ] **Step 1: Add gpu extra args**

`src/features/gpu.rs`:

```rust
pub fn extra_args(enabled: bool) -> Vec<String> {
    if enabled {
        vec!["--device".into(), "nvidia.com/gpu=all".into()]
    } else {
        vec![]
    }
}
```

In `ensure_container`, set `spec.extra = &gpu_args` derived from `cfg.gpu`.

- [ ] **Step 2: Build + commit**

```bash
cargo build
git add src/features/gpu.rs src/container/create.rs
git commit -m "feat(gpu): opt-in CDI device passthrough"
```

### Task 5.5: `rename` command

**Files:**
- Modify: `src/container/rename.rs`, `src/main.rs`

- [ ] **Step 1: Implement**

`src/container/rename.rs`:

```rust
use std::path::Path;

use crate::config::edit::set_name;
use crate::error::{Error, Result};
use crate::podman::runner::Podman;

pub fn rename(
    podman: &Podman,
    project: &Path,
    old_name: &str,
    new_name: &str,
) -> Result<()> {
    if podman.container_exists(new_name)? {
        return Err(Error::NameCollision(new_name.into(), project.to_path_buf()));
    }
    if podman.container_exists(old_name)? {
        podman.run(&["rename".into(), old_name.into(), new_name.into()])?;
    }
    let toml = project.join(".claude-sandbox.toml");
    if toml.exists() {
        set_name(&toml, new_name)?;
    }
    Ok(())
}
```

Wire into `main.rs`:

```rust
Cmd::Rename { new_name } => {
    claude_sandbox::container::rename::rename(&podman, &project, &name, &new_name)
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build
git add src/container/rename.rs src/main.rs
git commit -m "feat(rename): atomic podman rename + toml name update"
```

### Task 5.6: `migrate` command + registry

**Files:**
- Modify: `src/registry.rs`, `src/container/migrate.rs`, `src/main.rs`

- [ ] **Step 1: Implement minimal registry**

`src/registry.rs`:

```rust
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Registry {
    pub entries: BTreeMap<String, PathBuf>,
}

fn registry_path() -> PathBuf {
    paths::data_dir().join("registry.json")
}

pub fn load() -> Result<Registry> {
    let p = registry_path();
    if !p.exists() {
        return Ok(Registry::default());
    }
    let body = std::fs::read_to_string(p)?;
    Ok(serde_json::from_str(&body).unwrap_or_default())
}

pub fn save(reg: &Registry) -> Result<()> {
    let p = registry_path();
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::to_string_pretty(reg).unwrap_or_else(|_| "{}".into());
    std::fs::write(p, body)?;
    Ok(())
}

pub fn upsert(name: &str, path: &Path) -> Result<()> {
    let mut reg = load()?;
    reg.entries.insert(name.to_string(), path.to_path_buf());
    save(&reg)
}

pub fn remove(name: &str) -> Result<()> {
    let mut reg = load()?;
    reg.entries.remove(name);
    save(&reg)
}
```

Call `upsert` in `ensure_container` after a successful create, and `remove` in `down`.

- [ ] **Step 2: Implement migrate**

`src/container/migrate.rs`:

```rust
use std::path::Path;

use crate::config::edit::create_minimal;
use crate::error::Result;
use crate::registry;

pub fn migrate(container_name: &str, new_path: &Path) -> Result<()> {
    let toml = new_path.join(".claude-sandbox.toml");
    if !toml.exists() {
        create_minimal(&toml, container_name)?;
    }
    registry::upsert(container_name, new_path)
}
```

Wire into `main.rs`:

```rust
Cmd::Migrate { new_path } => {
    claude_sandbox::container::migrate::migrate(&name, &new_path)
}
```

- [ ] **Step 3: Build + commit**

```bash
cargo build
git add src/registry.rs src/container/migrate.rs src/container/create.rs src/container/lifecycle.rs src/main.rs
git commit -m "feat(registry, migrate): path↔name registry and orphan re-association"
```

### Task 5.7: Name-collision hash suffix

Spec §5.1 / §8: if two project paths derive the same name, the second container appends `-<8charhash>` automatically.

**Files:**
- Modify: `src/main.rs`, `src/project.rs`

- [ ] **Step 1: Add a stable hash helper**

Append to `src/project.rs`:

```rust
pub fn short_hash(path: &std::path::Path) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    path.hash(&mut h);
    format!("{:08x}", (h.finish() & 0xFFFF_FFFF) as u32)
}
```

- [ ] **Step 2: Resolve the actual container name before create**

In `start_or_shell` in `src/main.rs`, after deriving `name` from config-or-fallback, consult the registry:

```rust
let reg = claude_sandbox::registry::load()?;
let resolved = match reg.entries.get(&name) {
    Some(existing_path) if existing_path != project => {
        // Collision with a different project path; append hash suffix and persist in toml.
        let suffix = claude_sandbox::project::short_hash(project);
        let suffixed = format!("{name}-{suffix}");
        claude_sandbox::config::edit::set_name(&toml_path, &suffixed)?;
        suffixed
    }
    _ => name.clone(),
};
let name = resolved;
```

- [ ] **Step 3: Write a test for `short_hash` stability**

Append to `tests/naming.rs`:

```rust
use std::path::PathBuf;

use claude_sandbox::project::short_hash;

#[test]
fn short_hash_is_stable_and_eight_hex_chars() {
    let p = PathBuf::from("/home/u/p");
    let a = short_hash(&p);
    let b = short_hash(&p);
    assert_eq!(a, b);
    assert_eq!(a.len(), 8);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test --test naming
cargo build
git add src/project.rs src/main.rs tests/naming.rs
git commit -m "feat(naming): -<8char> suffix on registry-detected name collision"
```

---

## Phase 6 — Polish

### Task 6.1: `ls --orphans --size`

**Files:**
- Modify: `src/container/ls.rs`, `src/main.rs`

- [ ] **Step 1: Implement listing**

`src/container/ls.rs`:

```rust
use crate::error::Result;
use crate::podman::runner::Podman;
use crate::registry;

pub fn ls(podman: &Podman, orphans_only: bool, with_size: bool) -> Result<()> {
    let out = podman.run_json(&[
        "ps".into(),
        "-a".into(),
        "--filter".into(),
        "name=^cs-".into(),
        "--format".into(),
        "json".into(),
    ])?;
    let reg = registry::load()?;
    let arr = out.as_array().cloned().unwrap_or_default();
    for c in arr {
        let name = c.get("Names").and_then(|n| n.as_array()).and_then(|a| a.first())
            .and_then(|v| v.as_str()).unwrap_or("?");
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
            let sz = c
                .get("Size")
                .and_then(|s| s.as_str())
                .unwrap_or("?");
            format!("\t{sz}")
        } else {
            String::new()
        };
        let orphan_tag = if is_orphan { " [orphan]" } else { "" };
        println!("{name}\t{state}\t{path_disp}{size_disp}{orphan_tag}");
    }
    Ok(())
}
```

Wire into `main.rs`:

```rust
Cmd::Ls { orphans, size } => claude_sandbox::container::ls::ls(&podman, orphans, size),
```

- [ ] **Step 2: Build + commit**

```bash
cargo build
git add src/container/ls.rs src/main.rs
git commit -m "feat(ls): list cs-* with --orphans and --size"
```

### Task 6.2: `rebuild [--recreate]`

**Files:**
- Modify: `src/podman/image.rs`, `src/main.rs`

- [ ] **Step 1: Implement build**

`src/podman/image.rs`:

```rust
use std::path::Path;

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
    let res = podman.run_inherit(&[
        "build".into(),
        "-t".into(),
        "claude-sandbox:0.1".into(),
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
```

Wire into `main.rs`:

```rust
Cmd::Rebuild { recreate } => {
    claude_sandbox::podman::image::rebuild(&podman)?;
    if recreate {
        claude_sandbox::podman::image::recreate_all(&podman)?;
    }
    Ok(())
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build
git add src/podman/image.rs src/main.rs
git commit -m "feat(rebuild): build base image; --recreate prunes containers for recreate"
```

### Task 6.3: `init` command + first-run UX

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement init**

```rust
Cmd::Init { force } => {
    let cfg_dir = claude_sandbox::paths::config_dir();
    std::fs::create_dir_all(&cfg_dir)?;
    let dockerfile = cfg_dir.join("Dockerfile");
    let config_toml = cfg_dir.join("config.toml");
    if !dockerfile.exists() || force {
        std::fs::write(&dockerfile, include_str!("../assets/Dockerfile"))?;
        println!("wrote {}", dockerfile.display());
    }
    if !config_toml.exists() || force {
        std::fs::write(&config_toml, include_str!("../assets/default-config.toml"))?;
        println!("wrote {}", config_toml.display());
    }
    Ok(())
}
```

Note: `include_str!` paths are relative to the source file; ensure the build context includes `assets/`.

- [ ] **Step 2: Build + commit**

```bash
cargo build
git add src/main.rs
git commit -m "feat(init): scaffold ~/.config/claude-sandbox with Dockerfile + config"
```

### Task 6.4: `logs` command

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Implement**

```rust
Cmd::Logs => {
    podman.run_inherit(&[
        "logs".into(),
        "--tail".into(),
        "200".into(),
        "--follow".into(),
        name.clone(),
    ])
}
```

- [ ] **Step 2: Build + commit**

```bash
cargo build
git add src/main.rs
git commit -m "feat(logs): tail and follow container logs"
```

### Task 6.5: README

**Files:**
- Create: `README.md`

- [ ] **Step 1: Write a short README**

```markdown
# claude-sandbox

Run Claude Code in a rootless-Podman per-project sandbox.
Claude gets full `sudo` inside; your host is unaffected.

## Install

    make install     # builds and installs ~/.local/bin/claude-sandbox
    claude-sandbox init      # writes ~/.config/claude-sandbox/{Dockerfile,config.toml}
    claude-sandbox rebuild   # builds the base image (claude-sandbox:0.1)

## Use

    cd ~/some-project
    claude-sandbox             # creates a container on first run, launches `claude`
    claude-sandbox shell       # bash inside
    claude-sandbox stop        # preserves state
    claude-sandbox down        # destroys container + named home volume
    claude-sandbox ls          # list all cs-* containers

See [docs/2026-05-10-design.md](docs/2026-05-10-design.md) for the full design.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README with install and basic usage"
```

### Task 6.6: Error-message audit

**Files:**
- Modify: any module whose error string is unclear

- [ ] **Step 1: Walk through every variant in `src/error.rs` and make sure each carries enough context to act on**

Verify `error: podman exec failed` etc. always include the actual stderr from the failing command (already done in `runner.rs`). Spot-check by intentionally breaking things and reading the output.

- [ ] **Step 2: Commit if changed**

```bash
git add -p src/
git commit -m "chore(errors): clearer messages with command + context"
```

### Task 6.7: Final end-to-end check

- [ ] **Step 1: Run the full smoke sequence locally**

```bash
cd ~/Documents/projects/claude-sandbox && make install image
mkdir /tmp/cs-smoke && cd /tmp/cs-smoke && git init -q
claude-sandbox status         # absent
claude-sandbox shell          # drops you into bash inside cs-tmp-cs-smoke
# inside: cs status; whoami (root); apt-get install -y ripgrep; exit
claude-sandbox shell          # ripgrep still there (writable layer preserved)
claude-sandbox stop
claude-sandbox status         # stopped
claude-sandbox                # restarts and runs claude
# ^D
cs worktree add feat-a        # inside claude/shell
exit
claude-sandbox                # picker shows main + feat-a
claude-sandbox -w feat-a
exit
claude-sandbox down           # cleanup
```

- [ ] **Step 2: Tag the first release**

```bash
git tag v0.1.0
git log --oneline
```

---

## Open items deferred to a later iteration

The following items are in the spec's "open questions" section and are intentionally not implemented in this plan:

1. Bind-mounting host dotfiles into the container by default (current: opt-in only).
2. Multi-host distribution of `~/.config/claude-sandbox/` (current: layout is dotfiles-friendly, no automation).
3. Interactive prompt before first-run image build (current: silent-with-progress via `rebuild`).
4. Telemetry / debug capture beyond `-v` / `-vv` (current: none).
