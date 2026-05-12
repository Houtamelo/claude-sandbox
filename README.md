# claude-sandbox

Run Claude Code in a rootless-Podman per-project sandbox.
Claude gets full `sudo` inside; your host is unaffected.

## Host prerequisites

- Linux + Podman ≥ 4 + an apt-based Linux image as the build base.
- A working rootless Podman setup. On most distros (Ubuntu, Fedora, Arch) the
  package-manager defaults are sufficient.
- **openSUSE Tumbleweed only:** the default Podman ships `runc` and a kernel
  overlay driver, which fail on `--userns=keep-id` + bind mounts. Install
  `crun` and switch to `fuse-overlayfs`:

      sudo zypper install crun fuse-overlayfs
      mkdir -p ~/.config/containers
      cat > ~/.config/containers/storage.conf <<'EOF'
      [storage]
      driver = "overlay"

      [storage.options.overlay]
      mount_program = "/usr/bin/fuse-overlayfs"
      EOF

  Then reset existing storage so the new driver applies:

      podman ps -a -q | xargs -r podman rm -f --volumes
      rm -rf ~/.local/share/containers/storage

- SELinux-enabled hosts (Tumbleweed, Fedora, RHEL): no manual action needed —
  detected at runtime, container creation emits `--security-opt label=disable`
  automatically.

## Install

Packaged releases via apt/zypper are planned (Open Build Service); until those
land, build from source:

    cargo install --path .         # installs to ~/.cargo/bin/claude-sandbox
    claude-sandbox cfg             # interactive machine-setup wizard:
                                   #   - host UID
                                   #   - base image (default debian:trixie-slim)
                                   #   - GPU vendor + extra apt packages
                                   #   - claude flags + OAuth token
                                   #   - desktop integration (KDE auto-install)
                                   #   - editable defaults under ~/.config/...
    claude-sandbox rebuild         # builds the base image (claude-sandbox:0.1)

The binary ships its `Dockerfile` and default `config.toml` embedded via
`include_str!`, so a bare `cargo install` is self-sufficient. At runtime they
resolve in this order:

1. `~/.config/claude-sandbox/{Dockerfile,config.toml}` if you opted in to
   editable copies via the cfg wizard.
2. `/usr/share/claude-sandbox/{Dockerfile,config.toml}` if installed from a
   distro package.
3. The embedded fallback baked into the binary.

`make image` (still useful for dev) builds the container image directly from
`target/release/claude-sandbox` plus `assets/Dockerfile`. Every subcommand
except `cfg`, `init`, `--help`, `--version` requires that `cfg` has been run
at least once.

## Use

    cd ~/some-project
    claude-sandbox init        # marks the dir as a project (.claude-sandbox.toml)
    claude-sandbox             # creates a container on first run, launches `claude`
    claude-sandbox shell       # bash inside
    claude-sandbox goal "..."  # headless `/goal` session (set-and-forget)
    claude-sandbox stop        # preserves state
    claude-sandbox down        # destroys container + named home volume
    claude-sandbox ls          # list all claude-sandbox-managed containers

Editing `.claude-sandbox.toml` (per-project) or `~/.config/claude-sandbox/machine.toml`
(machine-wide) auto-triggers an image rebuild and/or container recreate on the
next start. Named home volume survives, so in-container `$HOME` state persists.

## Docs

- [docs/2026-05-10-design.md](docs/2026-05-10-design.md) — full design.
- [docs/flexibility-roadmap.md](docs/flexibility-roadmap.md) — which inputs
  are still hard-coded and what's planned to make them configurable.
