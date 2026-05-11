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
    };
    let args = create_args(&spec);
    // No cs-toml-hash=... entry. The discovery label cs-managed=1 is
    // still expected (that's unconditional).
    assert!(!args.iter().any(|a| a.starts_with("cs-toml-hash=")));
    assert!(args.contains(&"cs-managed=1".into()));
}

#[test]
fn exec_args_passes_command() {
    let args = exec_args("c", true, &["claude"]);
    assert_eq!(args, vec!["exec", "-it", "c", "claude"]);
}
