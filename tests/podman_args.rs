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
        toml_hash: None,
        machine_hash: None,
        oauth_hash: None,
        binary_hash: None,
        selinux: true,
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
    // SELinux opt-out so bind-mounts work on SELinux-enabled hosts
    // (Tumbleweed, Fedora). Container keeps rootless+userns isolation.
    assert!(args.contains(&"--security-opt".into()));
    assert!(args.contains(&"label=disable".into()));
    // Discovery label so `ls` can find every container regardless of name.
    assert!(args.contains(&"--label".into()));
    assert!(args.contains(&"cs-managed=1".into()));
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
        toml_hash: None,
        machine_hash: None,
        oauth_hash: None,
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    assert!(args.contains(&"/etc/foo:/etc/foo:ro".into()));
    assert!(args.contains(&"--publish".into()));
    assert!(args.contains(&"5173:5173".into()));
}

#[test]
fn create_args_includes_toml_hash_label_when_set() {
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &[],
        env: &[],
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
        toml_hash: Some("deadbeefcafef00d"),
        machine_hash: None,
        oauth_hash: None,
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    assert!(args.contains(&"cs-toml-hash=deadbeefcafef00d".into()));
}

#[test]
fn create_args_omits_toml_hash_label_when_none() {
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &[],
        env: &[],
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
        toml_hash: None,
        machine_hash: None,
        oauth_hash: None,
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    // No cs-toml-hash=... entry. The discovery label cs-managed=1 is
    // still expected (that's unconditional).
    assert!(!args.iter().any(|a| a.starts_with("cs-toml-hash=")));
    assert!(args.contains(&"cs-managed=1".into()));
}

#[test]
fn create_args_includes_oauth_hash_label_when_set() {
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &[],
        env: &[],
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
        toml_hash: None,
        machine_hash: None,
        oauth_hash: Some("0123456789abcdef"),
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    assert!(args.contains(&"cs-oauth-hash=0123456789abcdef".into()));
}

#[test]
fn create_args_includes_binary_hash_label_when_set() {
    // Binary-only changes (new podman-create flags, e.g. the NVIDIA
    // --group-add keep-groups fix) slip past every other recreate gate
    // because the user's config files haven't moved. The cs-binary-hash
    // label fingerprints the binary itself so the gate fires on
    // cargo-install or any other binary swap.
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &[],
        env: &[],
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
        toml_hash: None,
        machine_hash: None,
        oauth_hash: None,
        binary_hash: Some("cafef00ddeadbeef"),
        selinux: true,
    };
    let args = create_args(&spec);
    assert!(
        args.contains(&"cs-binary-hash=cafef00ddeadbeef".into()),
        "expected cs-binary-hash label in args: {args:?}"
    );
}

#[test]
fn create_args_does_not_emit_userns_keep_id() {
    // Reverted from 7d5a6e8: --userns=keep-id triggers a malformed OCI
    // spec on Tumbleweed + podman 5.8.2 + crun 1.27.1 ("readlink ``"
    // error, container won't start). Guard against accidental re-add
    // until that upstream bug is resolved.
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &[],
        env: &[],
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
        toml_hash: None,
        machine_hash: None,
        oauth_hash: None,
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    assert!(
        !args.iter().any(|a| a == "--userns"),
        "--userns must NOT be in args (keep-id broken on Tumbleweed+podman 5.8.2): {args:?}"
    );
}

#[test]
fn create_args_emits_umask_0002() {
    // Container's init process must default to umask 002 so files
    // created by agent processes (which run via `podman exec`, NOT
    // an interactive shell, so they DON'T source ~/.bashrc) come
    // out mode 0664 / 0775 — group-writable. Without this, the host
    // user can't edit files the in-container agent creates, even
    // though the file's group resolves to the host user's primary
    // group via userns translation of container GID 0.
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &[],
        env: &[],
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
        toml_hash: None,
        machine_hash: None,
        oauth_hash: None,
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    let pos = args
        .iter()
        .position(|a| a == "--umask")
        .expect("expected --umask flag in args");
    assert_eq!(
        args.get(pos + 1).map(|s| s.as_str()),
        Some("0002"),
        "umask value must be 0002 (group-writable); got: {:?}",
        args.get(pos + 1)
    );
}

#[test]
fn create_args_omits_binary_hash_label_when_none() {
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x",
        image: "i:1",
        volumes: &[],
        env: &[],
        network: "bridge",
        ports: &[],
        workdir: &workdir,
        extra: &[],
        toml_hash: None,
        machine_hash: None,
        oauth_hash: None,
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    assert!(!args.iter().any(|a| a.starts_with("cs-binary-hash=")));
}

#[test]
fn create_args_emits_selinux_optout_when_enabled() {
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x", image: "i:1",
        volumes: &[], env: &[], network: "bridge", ports: &[],
        workdir: &workdir, extra: &[],
        toml_hash: None, machine_hash: None, oauth_hash: None,
        binary_hash: None,
        selinux: true,
    };
    let args = create_args(&spec);
    assert!(args.contains(&"--security-opt".into()), "missing --security-opt: {args:?}");
    assert!(args.contains(&"label=disable".into()), "missing label=disable: {args:?}");
}

#[test]
fn create_args_omits_selinux_optout_when_disabled() {
    // Non-SELinux hosts (Ubuntu, Mint, vanilla Arch) get a cleaner
    // podman invocation. The label=disable flag is at best a no-op
    // there, at worst a deprecation warning on older podman.
    let workdir = PathBuf::from("/work");
    let spec = CreateSpec {
        name: "x", image: "i:1",
        volumes: &[], env: &[], network: "bridge", ports: &[],
        workdir: &workdir, extra: &[],
        toml_hash: None, machine_hash: None, oauth_hash: None,
        binary_hash: None,
        selinux: false,
    };
    let args = create_args(&spec);
    assert!(
        !args.contains(&"--security-opt".into()),
        "--security-opt should not appear when selinux=false; got {args:?}"
    );
    assert!(!args.contains(&"label=disable".into()));
    // Discovery label is still unconditional.
    assert!(args.contains(&"cs-managed=1".into()));
}

#[test]
fn exec_args_passes_command() {
    let args = exec_args("c", true, &["claude"]);
    assert_eq!(args, vec!["exec", "-it", "c", "claude"]);
}
