# Flexibility roadmap

Tracking which inputs to claude-sandbox are still hard-coded and what's planned
to make them configurable. Goal: shrink the surface area that's specific to
the original author's environment (openSUSE Tumbleweed + UID 1000 + NVIDIA +
Max OAuth subscription) so the project is useful to other people.

Status legend: ✅ done · ▶ in progress · ⏸ planned · 🤔 deferred

---

## ✅ Done

| Input | Mechanism |
|---|---|
| Host UID | `[host] uid` in `~/.config/claude-sandbox/machine.toml`, captured by `claude-sandbox cfg`, plumbed through Dockerfile `ARG HOSTUID`. Auto-rebuild on change via `cs-machine-hash` label. |
| Base image (`FROM`) | `[image] base` in `machine.toml`, default `debian:trixie-slim`. Templates Dockerfile `ARG BASE_IMAGE`. Must be apt-based; any apt-based image now works since Tailscale was extracted. |
| Claude auth (avoiding refresh-token rotation across shared `.credentials.json`) | Long-lived `CLAUDE_CODE_OAUTH_TOKEN` stored mode 600 at `~/.config/claude-sandbox/oauth_token`, validated against Anthropic's API at cfg-save AND container-start time, injected per-container. |
| SELinux opt-out unconditional | Runtime-detected via `/sys/fs/selinux/enforce`. `--security-opt label=disable` only emitted when SELinux is actually loaded; absent on Ubuntu/Mint/vanilla Arch. |
| GPU vendor (was NVIDIA-only) | `[gpu] vendor` in `machine.toml` — `none` (default), `nvidia`, `amd`, `intel`, or `custom`. cfg wizard probes (`/proc/driver/nvidia`, `/dev/kfd`, `/sys/class/drm/card0/device/vendor`) and pre-fills the prompt. `extra_args = [...]` is appended in every variant, including `none` and `custom`, as an escape hatch for driver-specific quirks. Per-project `gpu: bool` stays the toggle. |
| Tailscale baked into image | Removed. Was a ~30 MB always-installed package with a Debian-Trixie codename hardcoded in the apt repo URL — penalised non-users and blocked alternate bases. Users who want it follow [docs/recipes/tailscale.md](recipes/tailscale.md) (install via `.claude-sandbox.deps.sh`, run via `on_start` hooks, persist state via `[[mount]]`). Existing tomls with `[tailscale]` get a clean `unknown field` parse error pointing at the recipe. |
| Hardcoded apt package list | Split into two tiers. **Core** (`ca-certificates curl git sudo bash openssh-client acl pulseaudio-utils sound-theme-freedesktop gnupg`) is fixed in the Dockerfile — these are load-bearing for sandbox features (TLS, claude.ai installer, worktrees, sudo, hooks, SSH/GPG agent forwarding, ACLs, notification audio). **Extras** (`[image] extra_packages` in machine.toml, default = `build-essential pkg-config jq direnv`) is user-configurable via cfg wizard or direct edit. `extra_packages = []` skips the second RUN entirely for a minimal image. |
| SSH-key-only credential passthrough | Added `gpg_agent: bool` to per-project `.claude-sandbox.toml` (default false). When true and host `~/.gnupg/` exists, bind-mounts the directory rw at the matching in-container path. HOME mirroring means gpg auto-discovers its keyring + agent socket; signing / encryption / decryption all work. Exposes the keyring to the container (consistent with how `~/.claude` is treated). |
| `CLAUDE_FLAGS` hardcoded | `[claude] flags = [...]` in `machine.toml` (default `["--dangerously-skip-permissions"]` — fine inside the sandbox; the cfg wizard explains why). Per-project `claude_flags = [...]` in `.claude-sandbox.toml` fully replaces the machine-wide list (not appends — replacing means "use exactly these"; appending would force users to repeat the dangerous-skip baseline). The in-container `cs goal` reads from a `CS_CLAUDE_FLAGS` env var baked at container create so it uses the same flag set. |
| Dolphin servicemenu hardcodes konsole | Wizard detects `$XDG_CURRENT_DESKTOP`. KDE → prompt-and-install the servicemenu (skipping if already installed). Other DEs → message pointing at [docs/recipes/context-menu.md](recipes/context-menu.md) which covers GNOME (Nautilus scripts), XFCE (Thunar custom actions), Cinnamon (Nemo actions), MATE (Caja scripts), and minimal-WM aliases. Only KDE is auto-installed because there's no portable cross-DE ABI. |

---

## ⏸ Pending

### Critical (blocks non-author users)

*(All items in this tier are now done. Next major friction point is in Significant.)*

### Significant (constrains real use cases)

*(Empty. All Significant-tier items shipped.)*

### Minor (preference / convenience)

*(Empty. The pass is done.)*

---

## 🤔 Deferred / intentionally not configurable

- **Container user name `claude`** (`mounts.rs::CONTAINER_USER`, Dockerfile useradd, sudoers file naming, `grant_acls` setfacl commands). Purely internal — users don't type it, don't see it in any output unless they `whoami` inside the container, doesn't appear in any user-facing config. Parameterizing would touch the Dockerfile (new ARG), `grant_acls` (thread through five+ string interpolations), the constant, several tests, and the `cs goal` ACL code path for zero functional benefit. The name is opinionated but fixed.
- **Cross-distro support (Fedora / Arch / Alpine).** Would require branching every apt-step on package manager (apt/dnf/pacman/apk), per-distro sudo group differences, useradd flag differences. Real maintenance burden. The current "apt-based only" stance covers Debian/Ubuntu/Mint, which is the bulk of Linux dev hosts. Users on other distros can edit `assets/Dockerfile` directly.
- **macOS / Windows support.** Linux-only by design (rootless Podman + userns).
- **Docker support.** Not blocked — most things work via `podman → docker` aliasing — but no explicit testing or compatibility shims.
- **Multi-user containers.** Project-per-container is the mental model; sharing one big container across many projects is a different product.

---

## Convention identifiers (intentionally not configurable)

Some strings look hardcoded but shouldn't be parameterized because changing them buys nothing and complicates the codebase:

- `.claude-sandbox.toml`, `.claude-sandbox.deps.sh`, `.worktrees/` — project conventions.
- `CS_PROJECT_PATH`, `cs-managed=1`, `cs-toml-hash`, `cs-machine-hash`, `cs-oauth-hash`, `cs-<name>-home` — internal identifiers / labels.
- `claude-sandbox:0.1` image tag — overridable per-project via `image = "..."` in the project toml.
- `/CLAUDE.md` symlink target inside the container — internal contract.
