//! `build_grant_acls_script` assembles the bash command that
//! claude-sandbox runs as in-container root at every start to give
//! the in-container claude user write access to bind-mounted paths
//! AND fix group permissions so the host user can edit files the
//! agent creates.
//!
//! Two distinct sets of ACL ops:
//!   1. user:claude → rwx (recursive + default)  — claude can write
//!   2. group::rwx + mask::rwx (recursive + default)  — host can edit
//!
//! Without (2), agent-created files end up with `group::r-x` (the
//! umask + default-ACL-inheritance combo bakes the wrong default into
//! every subdir) and the host user — who matches the file's group
//! via userns mapping of container GID 0 → host primary group — gets
//! r-x and can't write.

use claude_sandbox::config::MountSpec;
use claude_sandbox::container::create::build_grant_acls_script;

#[test]
fn script_sets_user_claude_acl_recursively_for_bundled_paths() {
    let script = build_grant_acls_script(
        "claude",
        std::path::Path::new("/work/proj"),
        std::path::Path::new("/home/u"),
        &[],
    );
    assert!(
        script.contains("setfacl -R") && script.contains("u:claude:rwx"),
        "missing recursive user:claude:rwx setfacl: {script}"
    );
    assert!(
        script.contains("d:u:claude:rwx"),
        "missing default user:claude:rwx (for newly-created files): {script}"
    );
}

#[test]
fn script_grants_group_write_recursively_for_existing_files() {
    // Closes the "host can't edit files the agent created" bug. The
    // file's group resolves to host's primary group (via container
    // GID 0 → host GID 1000 userns mapping), so giving group rwx
    // means the host user can edit.
    let script = build_grant_acls_script(
        "claude",
        std::path::Path::new("/work/proj"),
        std::path::Path::new("/home/u"),
        &[],
    );
    assert!(
        script.contains("g::rwx"),
        "missing g::rwx — group can't write existing files: {script}"
    );
    assert!(
        script.contains("m::rwx"),
        "missing m::rwx — mask caps effective group perm below rwx: {script}"
    );
}

#[test]
fn script_grants_group_write_in_default_acl_for_new_files() {
    // Even with the current group::rwx fix, new files created inside
    // a dir without a permissive default group ACL would inherit
    // `default:group::r-x` from the parent's mode bits (which was the
    // root cause of the bug). Explicit `d:g::rwx` + `d:m::rwx` makes
    // future files default to group-write.
    let script = build_grant_acls_script(
        "claude",
        std::path::Path::new("/work/proj"),
        std::path::Path::new("/home/u"),
        &[],
    );
    assert!(
        script.contains("d:g::rwx"),
        "missing default:group::rwx — new files won't be group-writable: {script}"
    );
    assert!(
        script.contains("d:m::rwx"),
        "missing default:mask::rwx — mask caps default group perm below rwx: {script}"
    );
}

#[test]
fn script_covers_claude_state_dirs() {
    let script = build_grant_acls_script(
        "claude",
        std::path::Path::new("/work/proj"),
        std::path::Path::new("/home/u"),
        &[],
    );
    // ~/.claude (rw — settings/sessions), ~/.cache/claude-cli-nodejs,
    // ~/.cache/claude must all get the recursive ACL fix so the
    // container claude can write to them and the host can subsequently
    // edit anything the agent left behind.
    assert!(script.contains("/home/u/.claude"), "missing ~/.claude path: {script}");
    assert!(
        script.contains("/home/u/.cache/claude-cli-nodejs"),
        "missing ~/.cache/claude-cli-nodejs: {script}"
    );
    assert!(
        script.contains("/home/u/.cache/claude"),
        "missing ~/.cache/claude: {script}"
    );
}

#[test]
fn script_handles_user_declared_rw_mounts() {
    let mounts = vec![MountSpec {
        host: "~/.pulumi".into(),
        container: "/home/u/.pulumi".into(),
        ro: false,
        optional: false,
    }];
    let script = build_grant_acls_script(
        "claude",
        std::path::Path::new("/work/proj"),
        std::path::Path::new("/home/u"),
        &mounts,
    );
    assert!(
        script.contains("/home/u/.pulumi"),
        "user-declared rw mount missing from ACL script: {script}"
    );
}

#[test]
fn script_skips_ro_user_mounts() {
    // RO mounts: agent doesn't need write, and we'd rather not add an
    // ACL entry to a user-protected file unnecessarily.
    let mounts = vec![MountSpec {
        host: "~/.ssh".into(),
        container: "/home/u/.ssh".into(),
        ro: true,
        optional: false,
    }];
    let script = build_grant_acls_script(
        "claude",
        std::path::Path::new("/work/proj"),
        std::path::Path::new("/home/u"),
        &mounts,
    );
    assert!(
        !script.contains("/home/u/.ssh"),
        "ro mount must NOT be ACL'd: {script}"
    );
}

#[test]
fn script_ends_with_true_so_exit_code_is_zero() {
    // Every setfacl is followed by `2>/dev/null`, so failures are
    // silently swallowed, but bash's exit code is the last command's.
    // Trailing `true` guarantees podman-exec sees exit 0 even if the
    // last setfacl failed.
    let script = build_grant_acls_script(
        "claude",
        std::path::Path::new("/work/proj"),
        std::path::Path::new("/home/u"),
        &[],
    );
    assert!(
        script.trim_end().ends_with("true"),
        "script must end with `true` for exit-code stability: {script}"
    );
}
