use claude_sandbox::machine::{content_hash, ClaudeSpec, GpuSpec, HostSpec, ImageSpec, MachineConfig};

fn cfg(uid: u32) -> MachineConfig {
    MachineConfig {
        host: HostSpec { uid },
        image: ImageSpec::default(),
        gpu: GpuSpec::default(),
        claude: ClaudeSpec::default(),
    }
}

#[test]
fn hash_is_deterministic_across_calls() {
    let c = cfg(1000);
    let h1 = content_hash(&c);
    let h2 = content_hash(&c);
    assert_eq!(h1, h2, "FNV-1a hash must be deterministic");
    assert_eq!(h1.len(), 16, "hex digest should be 16 chars (u64)");
}

#[test]
fn hash_differs_when_uid_differs() {
    assert_ne!(
        content_hash(&cfg(1000)),
        content_hash(&cfg(1001)),
        "hash must change when host.uid changes — otherwise auto-rebuild won't fire"
    );
}

#[test]
fn hash_differs_when_base_image_differs() {
    let mut a = cfg(1000);
    let mut b = cfg(1000);
    b.image.base = "ubuntu:24.04".into();
    assert_ne!(content_hash(&a), content_hash(&b));
    // sanity: identical struct → identical hash
    a.image.base = "ubuntu:24.04".into();
    assert_eq!(content_hash(&a), content_hash(&b));
}

#[test]
fn default_image_base_is_debian_trixie_slim() {
    // The default is load-bearing: existing machine.toml files predating
    // the [image] section deserialize with this value. Changing it
    // silently would invalidate every existing user's container.
    assert_eq!(ImageSpec::default().base, "debian:trixie-slim");
}

#[test]
fn legacy_toml_without_gpu_section_parses() {
    let body = "[host]\nuid = 1000\n[image]\nbase = \"debian:trixie-slim\"\n";
    let c: MachineConfig = toml::from_str(body).expect("legacy toml should parse");
    assert_eq!(c.gpu, claude_sandbox::machine::GpuSpec::default());
}

#[test]
fn legacy_toml_without_image_section_parses() {
    // Back-compat: users who configured machine.toml before the [image]
    // section existed must keep loading cleanly. `#[serde(default)]` on
    // the field is what makes this work.
    let body = "[host]\nuid = 1000\n";
    let c: MachineConfig = toml::from_str(body).expect("legacy toml should parse");
    assert_eq!(c.host.uid, 1000);
    assert_eq!(c.image, ImageSpec::default());
}

#[test]
fn default_extra_packages_is_the_curated_set() {
    // Guards the author-opinionated list. Changing this default
    // silently rebuilds every existing user's image with a different
    // package set on next start — make it intentional.
    let default = ImageSpec::default().extra_packages;
    assert_eq!(default, vec!["build-essential", "pkg-config", "jq", "direnv"]);
}

#[test]
fn extra_packages_can_be_emptied() {
    // Users opting for a minimal image set `extra_packages = []`. Must
    // parse cleanly and the EXTRA_PACKAGES build arg ends up empty,
    // which the Dockerfile's `if [ -n "$EXTRA_PACKAGES" ]` guard skips.
    let body = "[host]\nuid = 1000\n[image]\nbase = \"debian:trixie-slim\"\nextra_packages = []\n";
    let c: MachineConfig = toml::from_str(body).expect("parse");
    assert!(c.image.extra_packages.is_empty());
}

#[test]
fn extra_packages_extends_default() {
    let body = "[host]\nuid = 1000\n[image]\nbase = \"x\"\nextra_packages = [\"vim\", \"tmux\"]\n";
    let c: MachineConfig = toml::from_str(body).expect("parse");
    assert_eq!(c.image.extra_packages, vec!["vim", "tmux"]);
}

#[test]
fn legacy_image_section_without_extra_packages_keeps_default() {
    // Back-compat: pre-extra_packages tomls have only `base` under
    // [image]. The deserializer must fill in the default extras.
    let body = "[host]\nuid = 1000\n[image]\nbase = \"debian:trixie-slim\"\n";
    let c: MachineConfig = toml::from_str(body).expect("parse");
    assert_eq!(c.image.extra_packages, ImageSpec::default().extra_packages);
}

#[test]
fn default_claude_flags_is_dangerously_skip() {
    // Guards the safety baseline. Users can override (per-project or by
    // editing machine.toml) but the OOTB default must keep the flag on.
    let s = claude_sandbox::machine::ClaudeSpec::default();
    assert_eq!(s.flags, vec!["--dangerously-skip-permissions"]);
}

#[test]
fn legacy_toml_without_claude_section_keeps_default() {
    let body = "[host]\nuid = 1000\n";
    let c: MachineConfig = toml::from_str(body).expect("parse");
    assert_eq!(c.claude, claude_sandbox::machine::ClaudeSpec::default());
}

#[test]
fn claude_flags_can_be_extended() {
    let body = r#"
[host]
uid = 1000
[claude]
flags = ["--dangerously-skip-permissions", "--model", "claude-opus-4-7"]
"#;
    let c: MachineConfig = toml::from_str(body).expect("parse");
    assert_eq!(
        c.claude.flags,
        vec!["--dangerously-skip-permissions", "--model", "claude-opus-4-7"]
    );
}

#[test]
fn claude_flags_can_be_emptied() {
    // Empty list = user explicitly wants no flags (e.g. they want
    // the in-app permission UX back). The wizard supports this via
    // a blank input.
    let body = "[host]\nuid = 1000\n[claude]\nflags = []\n";
    let c: MachineConfig = toml::from_str(body).expect("parse");
    assert!(c.claude.flags.is_empty());
}

#[test]
fn config_round_trips_through_toml() {
    let mut c = cfg(1234);
    c.image.base = "linuxmintd/mint22-amd64".into();
    let s = toml::to_string(&c).expect("serialize");
    let back: MachineConfig = toml::from_str(&s).expect("deserialize");
    assert_eq!(c, back);
}

#[test]
fn unknown_fields_rejected() {
    // deny_unknown_fields keeps typos from silently being ignored.
    let bad = "[host]\nuid = 1000\nunused_field = true\n";
    assert!(
        toml::from_str::<MachineConfig>(bad).is_err(),
        "unknown fields should fail to parse"
    );
}

#[test]
fn missing_required_field_rejected() {
    let bad = "[host]\n"; // no uid
    assert!(
        toml::from_str::<MachineConfig>(bad).is_err(),
        "missing host.uid should fail to parse"
    );
}
