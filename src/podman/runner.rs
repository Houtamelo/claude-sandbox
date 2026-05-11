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
        let args: Vec<String> = vec![
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
