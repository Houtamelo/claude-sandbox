//! Shared helpers for the e2e integration suite.
//!
//! All e2e tests are gated on `CLAUDE_SANDBOX_E2E=1` and require the
//! `claude-sandbox:0.1` image to be pre-built (`claude-sandbox rebuild`).
//!
//! Each test gets its own [`Sandbox`] backed by a tempdir; the container
//! is automatically destroyed on drop, including its named home volume.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use tempfile::TempDir;

pub const IMAGE: &str = "claude-sandbox:0.1";

/// Return true if `CLAUDE_SANDBOX_E2E=1`.
pub fn e2e_enabled() -> bool {
    std::env::var("CLAUDE_SANDBOX_E2E").ok().as_deref() == Some("1")
}

/// Return true if the test image exists locally.
pub fn image_exists() -> bool {
    Command::new("podman")
        .args(["image", "exists", IMAGE])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Shorthand: skip a test cleanly if e2e isn't enabled OR the image is missing.
/// Returns `true` if the test should be skipped (caller should `return` immediately).
pub fn should_skip(name: &str) -> bool {
    if !e2e_enabled() {
        eprintln!("[skip] {name}: set CLAUDE_SANDBOX_E2E=1 to run");
        return true;
    }
    if !image_exists() {
        eprintln!("[skip] {name}: image {IMAGE} not built — run `claude-sandbox rebuild`");
        return true;
    }
    false
}

/// Run `podman` with the given args. Returns the [`Output`] regardless of exit status.
pub fn podman(args: &[&str]) -> Output {
    Command::new("podman")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("podman not on PATH")
}

/// Run a one-shot command inside a fresh container of [`IMAGE`] and return its [`Output`].
/// Container is `--rm`'d. Useful for image-sanity tests that don't need a persistent sandbox.
pub fn run_in_image(cmd: &[&str]) -> Output {
    let mut full = vec!["run", "--rm", "--entrypoint", "bash", IMAGE, "-c"];
    let joined = cmd.join(" ");
    full.push(&joined);
    podman(&full)
}

/// A test sandbox: tempdir-backed project + lifetime-managed container.
///
/// On drop, runs `podman rm -f --volumes <name>` so each test self-cleans
/// even if it panics. Run cargo with `--test-threads=N` if you need to
/// throttle, but tests are independent (unique container names per
/// tempdir path).
pub struct Sandbox {
    pub dir: TempDir,
    pub name: String,
}

impl Sandbox {
    /// Create a tempdir with a `.git` directory and derive the container name.
    pub fn new() -> Self {
        let dir = TempDir::new().expect("tempdir");
        std::fs::create_dir(dir.path().join(".git")).expect("mkdir .git");
        let name = container_name_for(dir.path());
        Self { dir, name }
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Path to the binary under test (`target/debug/claude-sandbox`).
    pub fn bin() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_claude-sandbox"))
    }

    /// Invoke the host binary against this sandbox's project directory.
    pub fn cli(&self, args: &[&str]) -> Output {
        Command::new(Self::bin())
            .args(args)
            .current_dir(self.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .expect("spawn claude-sandbox")
    }

    /// `podman exec` a command in this sandbox's container (non-interactive).
    pub fn podman_exec(&self, cmd: &[&str]) -> Output {
        let mut args: Vec<&str> = vec!["exec", &self.name];
        args.extend_from_slice(cmd);
        podman(&args)
    }

    /// Return the parsed `podman inspect` JSON for this sandbox's container.
    /// Panics if the container doesn't exist.
    pub fn inspect(&self) -> serde_json::Value {
        let out = podman(&["inspect", "--format", "{{json .}}", &self.name]);
        assert!(
            out.status.success(),
            "podman inspect failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("inspect json")
    }

    pub fn container_exists(&self) -> bool {
        Command::new("podman")
            .args(["container", "exists", &self.name])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        // Best-effort cleanup. Stop + remove container with its volumes,
        // and clear the named home volume separately in case it survived.
        let _ = Command::new("podman")
            .args(["rm", "--force", "--volumes", &self.name])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let vol = format!("cs-{}-home", self.name);
        let _ = Command::new("podman")
            .args(["volume", "rm", "--force", &vol])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Derive the container name from a tempdir path the same way the
/// production binary does (path components below `$HOME`, lowercased,
/// `/`/whitespace → `-`).
///
/// Tempdirs typically live under `/tmp` (outside `$HOME`), so the name
/// gets the `root-tmp-...` prefix per `derive_name`'s outside-home branch.
fn container_name_for(path: &Path) -> String {
    let home = dirs::home_dir().expect("HOME");
    claude_sandbox::project::derive_name(path, &home)
}
