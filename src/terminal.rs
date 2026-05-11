//! Terminal-window title management.
//!
//! Emits the standard `OSC 0` escape sequence (`ESC ] 0 ; <title> BEL`)
//! which sets both the window title and icon title on most modern
//! terminals (konsole, gnome-terminal, xterm, alacritty, kitty, iTerm2,
//! Windows Terminal, ...). Silently bails if stderr isn't a TTY so we
//! don't spew escape codes into pipes, log files, or CI captures.

use std::io::{IsTerminal, Write};
use std::path::Path;

/// Set the terminal window title to `Claude - <project-basename> - <worktree>`.
/// `worktree = None` is rendered as `main`. No-op when stderr isn't a TTY.
pub fn set_title(project: &Path, worktree: Option<&str>) {
    let mut stderr = std::io::stderr();
    if !stderr.is_terminal() {
        return;
    }
    let project_name = project
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.display().to_string());
    let wt = worktree.unwrap_or("main");
    // OSC 0: set both window and icon title. BEL terminator (\x07) is the
    // historical xterm form and is the most broadly supported; ST (\x1b\\)
    // is the strict-VT100 form but some terminals only do BEL.
    let _ = write!(stderr, "\x1b]0;Claude - {project_name} - {wt}\x07");
    let _ = stderr.flush();
}

/// Build the OSC 0 byte sequence as it would be written to stderr. Pure
/// function for unit-testing the format without touching the terminal.
pub fn title_sequence(project: &Path, worktree: Option<&str>) -> String {
    let project_name = project
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.display().to_string());
    let wt = worktree.unwrap_or("main");
    format!("\x1b]0;Claude - {project_name} - {wt}\x07")
}
