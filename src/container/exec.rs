use std::ffi::CString;

use nix::unistd::execvp;

use crate::debug1;
use crate::error::{Error, Result};

pub fn exec_into(name: &str, cmd: &[&str]) -> Result<()> {
    let argv = build_argv(name, cmd)?;
    debug1!("execvp: {}", argv.join(" "));
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
