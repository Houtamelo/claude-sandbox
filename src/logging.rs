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

/// Phase header for the bootstrap pipeline — prints `==> <msg>` to stderr
/// at every verbosity level. Use sparingly: only at the start of an
/// actual bootstrap phase the user would benefit from seeing
/// ("Loading configuration", "Creating container", "Running setup
/// hooks", etc.), not for every podman invocation.
#[macro_export]
macro_rules! step {
    ($($arg:tt)*) => {
        eprintln!("==> {}", format!($($arg)*));
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
