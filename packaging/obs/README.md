# OBS packaging

This directory holds the recipes Open Build Service (OBS) needs to
produce `.deb` and `.rpm` packages of `claude-sandbox`. Users then
add a single repo URL (per their distro) and install with
`apt install claude-sandbox` / `zypper in claude-sandbox`.

## Files

| File | Purpose |
|---|---|
| `_service` | OBS source-fetch recipe. Pulls the latest annotated `v*` tag from GitHub and produces the source tarball. |
| `claude-sandbox.spec` | RPM build recipe (Tumbleweed / Leap / Fedora). |
| `debian.control` / `debian.rules` / `debian.changelog` / `debian.compat` | Debian build recipe family (Debian / Ubuntu / Mint). |
| `../release.sh` | Local-side release ritual. Vendors crates, builds the `.tar.gz`, prints the OBS upload commands. |

## One-time OBS setup

1. Sign in at <https://build.opensuse.org> with your OpenID / GitHub.
2. Create a home project: `home:houtamelo`.
3. Create a package inside it: `home:houtamelo/claude-sandbox`.
4. Enable build targets in the package's "Repositories" tab:
   - `openSUSE_Tumbleweed`
   - `openSUSE_Leap_15.6`
   - `Fedora_42`
   - `Fedora_41`
   - `Debian_13`
   - `Ubuntu_24.04`
   - `Ubuntu_26.04` *(when OBS publishes the target — likely a few months after the April 2026 release)*

   Architecture: `x86_64`. Add `aarch64` only when there's a user asking.

5. Upload the recipe files (one-time):

   ```bash
   osc co home:houtamelo claude-sandbox
   cd home:houtamelo/claude-sandbox
   cp /path/to/repo/packaging/obs/{_service,claude-sandbox.spec,debian.*} .
   osc add _service claude-sandbox.spec debian.*
   osc commit -m "initial recipe"
   ```

## Per-release ritual

```bash
# In the claude-sandbox repo:
git tag -a v0.3.0 -m "release notes"
./packaging/release.sh

# Output: packaging/obs/claude-sandbox-0.3.0.tar.gz

# In your osc checkout of home:houtamelo/claude-sandbox:
cp /path/to/repo/packaging/obs/claude-sandbox-0.3.0.tar.gz .
osc add claude-sandbox-0.3.0.tar.gz

# Also bump the Version: line in claude-sandbox.spec and prepend an
# entry to debian.changelog (the script could automate this but
# automation hides what should be a deliberate copy-edit).
osc commit -m "release 0.3.0"

# Then push the tag so the GitHub release matches:
git push origin v0.3.0
```

OBS rebuilds every enabled target on commit. Failures land in
the package dashboard.

## How users install

### openSUSE Tumbleweed / Leap

```bash
zypper ar -f \
  https://download.opensuse.org/repositories/home:/houtamelo/openSUSE_Tumbleweed/home:houtamelo.repo
zypper in claude-sandbox
```

### Fedora

```bash
dnf config-manager --add-repo \
  https://download.opensuse.org/repositories/home:/houtamelo/Fedora_42/home:houtamelo.repo
dnf install claude-sandbox
```

### Debian / Ubuntu / Mint

OBS-hosted apt repos need the signing key imported separately. Example
for Debian 13:

```bash
sudo mkdir -p /etc/apt/keyrings
curl -fsSL https://download.opensuse.org/repositories/home:/houtamelo/Debian_13/Release.key \
  | gpg --dearmor | sudo tee /etc/apt/keyrings/houtamelo.gpg > /dev/null

echo "deb [signed-by=/etc/apt/keyrings/houtamelo.gpg] \
  https://download.opensuse.org/repositories/home:/houtamelo/Debian_13/ /" \
  | sudo tee /etc/apt/sources.list.d/houtamelo.list

sudo apt update && sudo apt install claude-sandbox
```

Swap `Debian_13` for the matching target tag (`Ubuntu_24.04`,
`Ubuntu_26.04`, etc.). The exact URL pattern is visible at
<https://download.opensuse.org/repositories/home:/houtamelo/>.

## After installing

The package lays down:

| Path | Contents |
|---|---|
| `/usr/bin/claude-sandbox` | The binary. |
| `/usr/share/claude-sandbox/Dockerfile` | Default sandbox Dockerfile. |
| `/usr/share/claude-sandbox/config.toml` | Default per-project config. |
| `/usr/share/claude-sandbox/CLAUDE.md` | Sandbox-self-awareness doc baked into the image. |
| `/usr/share/kio/servicemenus/open-in-claude-sandbox.desktop` | KDE Dolphin right-click entry. |

The binary's three-tier lookup means the user can drop edited copies of
`Dockerfile` / `config.toml` into `~/.config/claude-sandbox/` to override
the system defaults without touching `/usr/share`. The cfg wizard
(`claude-sandbox cfg`) offers to do this on first run.

## Tumbleweed runtime note

Tumbleweed's default `runc` + kernel overlay storage driver combo
fails on `--userns=keep-id`. The package declares `fuse-overlayfs`
as a hard `Requires` so it's pulled in automatically, and
`Recommends: crun` so most users get the working stack.

`storage.conf` still needs a one-time edit because `~/.config/containers/`
is per-user, not something the package can write. The cfg wizard
prints a hint when this is detected. The README in the upstream repo
has the literal `storage.conf` snippet.

## Why vendored?

OBS build chroots are network-isolated — `cargo` can't reach
crates.io at build time. `packaging/release.sh` runs `cargo vendor`
on the maintainer's host and stuffs `vendor/` + `.cargo/config.toml`
into the source tarball so the spec/rules can call
`cargo build --offline --frozen` inside the chroot. Trade-off: the
source tarball is large (tens of MB compressed) but the build is
hermetic and reproducible.
