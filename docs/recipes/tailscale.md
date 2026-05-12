# Recipe: Tailscale inside a claude-sandbox container

The built-in Tailscale integration was removed on 2026-05-12. It bloated the
image for everyone (~30 MB of `tailscaled` + repo metadata) and hardcoded the
Debian Trixie codename in the apt repo URL, blocking other apt-based bases.

Users who do want each project on its own tailnet device can wire it up
manually via the same hooks the built-in version used. The recipe below
covers everything the integrated feature did.

## How it works

Each sandbox container runs its own `tailscaled` in **userspace networking
mode** (no kernel TUN device, no `CAP_NET_ADMIN` required). The daemon's
state — including the machine key that identifies this device on the
tailnet — is persisted in a per-project host directory so re-auth isn't
needed on every container churn. The Tailscale device name is the
sandbox's container name, so each project shows up as its own host in the
tailnet admin console.

## One-time host setup

```bash
# 1. Pick a directory to hold the per-project Tailscale state. Mode 700;
#    contains the machine's private key.
mkdir -p ~/.config/claude-sandbox/tailscale-state/YOUR-PROJECT
chmod 700 ~/.config/claude-sandbox/tailscale-state/YOUR-PROJECT

# 2. Generate an auth key at https://login.tailscale.com/admin/settings/keys
#    Reusable + ephemeral is fine; the daemon only needs it once, then
#    persists its own machine key in the state dir.
export TS_AUTHKEY="tskey-auth-..."

# Optionally persist the export in ~/.bashrc / ~/.zshrc / direnv / your
# password manager so future `claude-sandbox` invocations pick it up.
```

## Project files

### `.claude-sandbox.toml` additions

```toml
# Forward the auth key from the host's env into the container's env. Read
# once by `tailscale up`; never written to disk inside the container.
env_passthrough = ["TS_AUTHKEY"]

# Persistent Tailscale state. Replace YOUR-PROJECT with the same path
# component you used in the mkdir step above — must be unique per project
# so machine keys don't collide.
[[mount]]
host = "~/.config/claude-sandbox/tailscale-state/YOUR-PROJECT"
container = "/var/lib/tailscale"
ro = false

# Start the daemon then authenticate. Both hooks are idempotent on
# re-runs (the `pidof` guard skips spawning a second daemon; `|| true`
# on `tailscale up` survives already-authenticated containers).
on_start = [
    "pidof tailscaled >/dev/null || (tailscaled --tun=userspace-networking --statedir=/var/lib/tailscale > /var/log/tailscaled.log 2>&1 &)",
    'tailscale up --authkey="${TS_AUTHKEY}" --hostname="$CS_PROJECT_NAME" --accept-dns=false --accept-routes=false || true',
]
```

### `.claude-sandbox.deps.sh`

```sh
#!/usr/bin/env bash
# Installs the Tailscale apt repo + package inside the container. Runs
# once on container creation as root; re-runs cheaply on recreate.
#
# Adjust the URL codename for non-Trixie bases:
#   - Debian Bookworm: pkgs.tailscale.com/stable/debian/bookworm.*
#   - Ubuntu Noble (24.04): pkgs.tailscale.com/stable/ubuntu/noble.*
#   - Ubuntu Jammy (22.04): pkgs.tailscale.com/stable/ubuntu/jammy.*
#   - Mint 22: same as Ubuntu Noble
set -e
curl -fsSL https://pkgs.tailscale.com/stable/debian/trixie.noarmor.gpg \
  -o /usr/share/keyrings/tailscale-archive-keyring.gpg
curl -fsSL https://pkgs.tailscale.com/stable/debian/trixie.tailscale-keyring.list \
  -o /etc/apt/sources.list.d/tailscale.list
apt-get update
apt-get install -y --no-install-recommends tailscale
```

## What each piece does

### `--tun=userspace-networking`

Rootless containers don't have `CAP_NET_ADMIN` or `/dev/net/tun`, so the
default kernel-TUN mode fails. Userspace networking embeds gVisor's netstack
inside `tailscaled` — packets are routed entirely in userspace, no kernel
TUN device required. Slightly slower than kernel TUN at high throughput
but invisible for the typical SSH / git / web traffic a dev container does.

### `--statedir=/var/lib/tailscale` + the host bind

Without persistence, `tailscaled` keeps its machine key in the container's
writable layer — destroyed on `claude-sandbox down` AND on every
auto-recreate triggered by a config change. With the bind mount, the
state survives anything that doesn't touch the host directory itself.
Per-project dir ensures two projects on the same machine don't accidentally
share a tailnet device.

### `--hostname="$CS_PROJECT_NAME"`

`$CS_PROJECT_NAME` is the resolved container name (e.g.
`documents-projects-scone`), exported into every on_start hook. Using it
as the Tailscale hostname means each project shows up as its own device
in the tailnet admin console. Container names already have collision
suffixes, so two projects with the same basename in different parents
don't fight over the name.

### `--accept-dns=false`

Stops `tailscaled` from rewriting `/etc/resolv.conf` to point at MagicDNS.
The container's DNS already works via the host's resolver — letting
Tailscale clobber it leads to "DNS works on the host but not in the
container" debugging that's painful to track down. Flip to `true` if you
want MagicDNS for `*.ts.net` resolution inside the container.

### `--accept-routes=false`

Don't install routes for subnet routers advertised by other tailnet
nodes. The container reaches its peers directly; it doesn't tunnel
through someone else's `--advertise-routes`. Less surprise; flip to
`true` if you need subnet routing.

### `pidof tailscaled >/dev/null || (... &)` guard

`on_start` hooks run on every container start (including `stop` →
`start` without recreate). Without the guard, repeated starts would
spawn multiple daemons. `pidof` returns zero if a process by that name
exists; the `||` short-circuits the daemon launch when one's already
running.

### `&` + log redirect

`tailscaled` is a long-running daemon. `on_start` hooks must return so
the container can finish booting. The `&` backgrounds the daemon;
`> /var/log/tailscaled.log 2>&1` captures stdout/stderr so you have
something to grep when authentication fails.

### `TS_AUTHKEY` via `env_passthrough`

Auth keys are sensitive — `env_passthrough` forwards the value from the
host's environment into the container's env without ever writing it to
a file in the project. Once `tailscale up` consumes the key, the daemon
has its persistent machine key in `/var/lib/tailscale` and the auth key
isn't needed again until you `tailscale logout` or rotate the state dir.

## Verifying

After `claude-sandbox` starts:

```bash
# Inside the container:
tailscale status     # should list peers
tailscale ip -4      # your tailnet IPv4
```

On the host or another tailnet device:

```bash
tailscale status | grep YOUR-PROJECT-CONTAINER-NAME
```

If the daemon is up but auth failed, check `/var/log/tailscaled.log`
inside the container.

## Resetting / rotating

- **Re-auth from scratch:** delete the host state dir and re-run.
  ```bash
  rm -rf ~/.config/claude-sandbox/tailscale-state/YOUR-PROJECT/*
  ```
- **Remove device from tailnet admin:** find it in
  https://login.tailscale.com/admin/machines and click "Delete".
- **Stop Tailscale just for this session:** remove the `on_start`
  entries from `.claude-sandbox.toml` (auto-recreate; daemon won't
  start next time).
