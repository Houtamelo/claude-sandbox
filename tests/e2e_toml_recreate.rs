//! When `.claude-sandbox.toml` changes between `claude-sandbox` invocations,
//! the existing container must be rm'd and recreated so the new mounts /
//! env / ports / labels actually take effect (podman bakes them at create
//! time). The named home volume (`cs-<name>-home`) is preserved across
//! the recreate so in-container `$HOME` state survives.
//!
//! Guards regression for: "I edited my toml and nothing changed" / "I had
//! to remember to `down` before my changes applied".

mod common;

use std::path::Path;

use common::{should_skip, Sandbox};

#[test]
fn container_gets_toml_hash_label_at_create() {
    if should_skip("container_gets_toml_hash_label_at_create") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);

    let label = read_label(&sb, "cs-toml-hash");
    assert!(
        label.is_some() && !label.as_deref().unwrap_or("").is_empty(),
        "container missing cs-toml-hash label — auto-recreate-on-config-change \
         is broken without it. labels: {:?}",
        all_labels(&sb)
    );
}

#[test]
fn editing_toml_triggers_recreate_keeping_named_volume() {
    if should_skip("editing_toml_triggers_recreate_keeping_named_volume") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);

    let id_before = container_id(&sb);
    let hash_before = read_label(&sb, "cs-toml-hash").expect("hash label set");

    // Drop a marker into the named home volume so we can prove it
    // survives the recreate. The volume is mounted at the container's
    // HOME — write to a path we know is in the named volume's reach.
    let home = claude_sandbox::mounts::container_home();
    let marker = format!("{}/.cs-recreate-marker", home.display());
    let _ = common::podman(&["start", &sb.name]);
    let out = sb.podman_exec(&["bash", "-c", &format!("echo persist > {marker}")]);
    assert!(
        out.status.success(),
        "marker write failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // Stop so recreate can `rm -f` cleanly.
    let _ = common::podman(&["stop", &sb.name]);

    // Mutate the toml — anything that changes its bytes.
    append_to_toml(sb.path(), "\n# touched\n");

    // Re-enter prepare_container. Should detect the hash mismatch,
    // rm the container, and create a fresh one.
    create_via_lib(&sb);

    let id_after = container_id(&sb);
    assert_ne!(
        id_before, id_after,
        "container ID unchanged after toml edit — recreate didn't fire"
    );

    let hash_after = read_label(&sb, "cs-toml-hash").expect("hash label set on new container");
    assert_ne!(
        hash_before, hash_after,
        "toml-hash label didn't update on the recreated container"
    );

    // Named volume survives — the marker is still there.
    let _ = common::podman(&["start", &sb.name]);
    let out = sb.podman_exec(&["bash", "-c", &format!("cat {marker}")]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && stdout.trim() == "persist",
        "named home volume didn't survive the recreate. \
         expected `persist`, got stdout={stdout:?} stderr={:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unchanged_toml_does_not_recreate() {
    if should_skip("unchanged_toml_does_not_recreate") {
        return;
    }
    let sb = Sandbox::new();
    create_via_lib(&sb);
    let id_before = container_id(&sb);

    // No toml change: prepare_container should be a no-op on the
    // container itself.
    create_via_lib(&sb);
    let id_after = container_id(&sb);
    assert_eq!(
        id_before, id_after,
        "container was recreated even though toml is unchanged"
    );
}

// --- helpers ---

fn create_via_lib(sb: &Sandbox) {
    use claude_sandbox::config::{edit, load_merged};
    use claude_sandbox::container::create::{ensure_container, grant_acls, CreateOptions};
    use claude_sandbox::podman::runner::Podman;

    let toml = sb.path().join(".claude-sandbox.toml");
    if !toml.exists() {
        edit::create_minimal(&toml, &sb.name).expect("auto-create toml");
    }
    let cfg = load_merged(None, Some(&toml)).expect("load merged");
    let podman = Podman::discover().expect("podman");

    ensure_container(
        &podman,
        &CreateOptions {
            name: &sb.name,
            image: common::IMAGE,
            project_path: sb.path(),
            config: &cfg,
            machine_hash: None,
            oauth_hash: None,
            oauth_token: None,
        },
    )
    .expect("ensure_container");
    let _ = common::podman(&["start", &sb.name]);
    grant_acls(&podman, &sb.name, sb.path(), &[]).expect("grant_acls");
}

fn container_id(sb: &Sandbox) -> String {
    let out = common::podman(&["inspect", "--format", "{{.Id}}", &sb.name]);
    assert!(
        out.status.success(),
        "inspect failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn read_label(sb: &Sandbox, key: &str) -> Option<String> {
    let v = sb.inspect();
    v.get("Config")
        .and_then(|c| c.get("Labels"))
        .and_then(|l| l.get(key))
        .and_then(|s| s.as_str())
        .map(|s| s.to_string())
}

fn all_labels(sb: &Sandbox) -> serde_json::Value {
    sb.inspect()
        .get("Config")
        .and_then(|c| c.get("Labels"))
        .cloned()
        .unwrap_or(serde_json::Value::Null)
}

fn append_to_toml(project: &Path, s: &str) {
    let path = project.join(".claude-sandbox.toml");
    let existing = std::fs::read_to_string(&path).expect("read toml");
    std::fs::write(&path, existing + s).expect("write toml");
}
