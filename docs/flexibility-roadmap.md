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
| Base image (`FROM`) | `[image] base` in `machine.toml`, default `debian:trixie-slim`. Templates Dockerfile `ARG BASE_IMAGE`. Must be apt-based; Tailscale layer is the only thing that breaks on non-Debian-Trixie bases. |
| Claude auth (avoiding refresh-token rotation across shared `.credentials.json`) | Long-lived `CLAUDE_CODE_OAUTH_TOKEN` stored mode 600 at `~/.config/claude-sandbox/oauth_token`, validated against Anthropic's API at cfg-save AND container-start time, injected per-container. |
| SELinux opt-out unconditional | Runtime-detected via `/sys/fs/selinux/enforce`. `--security-opt label=disable` only emitted when SELinux is actually loaded; absent on Ubuntu/Mint/vanilla Arch. |

---

## ⏸ Pending

### Critical (blocks non-author users)

*(All items in this tier are now done. Next major friction point is in Significant.)*

### Significant (constrains real use cases)

- **GPU is NVIDIA-only.** `features::gpu::extra_args` returns `["--device", "nvidia.com/gpu=all"]`. AMD ROCm / Intel iGPU users get nothing.
  - *Proposal:* change `gpu = true` to `gpu = "nvidia" | "amd" | "intel" | "none"` (or `[gpu] vendor = "..."`) in per-project `.claude-sandbox.toml`. Branch on the value.
  - *Scope:* ~30 min.

- **Tailscale install layer hardcodes Debian-Trixie codename.** `pkgs.tailscale.com/stable/debian/trixie.*` URLs in the Dockerfile. Breaks rebuilds on any non-Debian-Trixie base.
  - *Proposal:* add `[image] with_tailscale: bool` (default true) — gates the entire install layer. Off-by-default skips it; users on alternate bases can disable it. Codename matching is option B / out of scope.
  - *Scope:* ~30 min.

- **Hardcoded apt package list.** `git sudo curl ca-certificates openssh-client build-essential pkg-config jq direnv acl pulseaudio-utils sound-theme-freedesktop` is one author's taste. No override.
  - *Proposal:* either trim to a minimum-viable set (`git sudo curl ca-certificates openssh-client acl`) and rely on per-project `setup = [...]` for the rest, OR expose `[image] extra_packages = [...]` build arg.
  - *Scope:* ~45 min depending on direction.

### Minor (preference / convenience)

- **`CLAUDE_FLAGS = ["--dangerously-skip-permissions"]` hardcoded.** Some users may want the prompt UX or to pass `--allowedTools`/`--model`.
  - *Proposal:* `claude_flags = ["..."]` in per-project toml, merged with the safety default rather than replacing it (or a `safe_mode = true` to drop the dangerous-skip).
  - *Scope:* ~20 min.

- **Container user name `claude` is hardcoded** (`mounts.rs::CONTAINER_USER`, Dockerfile, ACL commands). Doesn't collide with anything in practice but is opinionated.
  - *Proposal:* parameterize via build-arg `CONTAINER_USER=claude`. Purely cosmetic.
  - *Scope:* ~20 min.

- **Dolphin servicemenu uses `konsole`.** Not portable to other terminals.
  - *Proposal:* document the `Exec=` edit for other terminals; or detect $TERMINAL. Already optional via `make install-dolphin-menu`.
  - *Scope:* doc-only.

---

## 🤔 Deferred (intentionally out of scope)

- **Cross-distro support (Fedora / Arch / Alpine).** Would require branching every apt-step on package manager (apt/dnf/pacman/apk), per-distro Tailscale repo URLs, sudo group differences, useradd flag differences. Real maintenance burden. The current "apt-based only" stance covers Debian/Ubuntu/Mint, which is the bulk of Linux dev hosts.
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
