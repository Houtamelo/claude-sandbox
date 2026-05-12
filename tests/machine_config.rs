use claude_sandbox::machine::{content_hash, HostSpec, MachineConfig};

#[test]
fn hash_is_deterministic_across_calls() {
    let cfg = MachineConfig {
        host: HostSpec { uid: 1000 },
    };
    let h1 = content_hash(&cfg);
    let h2 = content_hash(&cfg);
    assert_eq!(h1, h2, "FNV-1a hash must be deterministic");
    assert_eq!(h1.len(), 16, "hex digest should be 16 chars (u64)");
}

#[test]
fn hash_differs_when_uid_differs() {
    let a = MachineConfig { host: HostSpec { uid: 1000 } };
    let b = MachineConfig { host: HostSpec { uid: 1001 } };
    assert_ne!(
        content_hash(&a),
        content_hash(&b),
        "hash must change when host.uid changes — otherwise auto-rebuild won't fire"
    );
}

#[test]
fn config_round_trips_through_toml() {
    let cfg = MachineConfig { host: HostSpec { uid: 1234 } };
    let s = toml::to_string(&cfg).expect("serialize");
    let back: MachineConfig = toml::from_str(&s).expect("deserialize");
    assert_eq!(cfg, back);
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
