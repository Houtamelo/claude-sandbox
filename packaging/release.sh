#!/usr/bin/env bash
# Local release ritual for OBS publishing. Run this on a clean tag,
# upload the resulting tarball + recipe files to OBS, push the tag.
#
# What it does:
#   1. Bails if the working tree is dirty (the tarball must reflect
#      committed state).
#   2. Reads the version from Cargo.toml; expects HEAD to be at an
#      annotated `v<version>` tag.
#   3. Runs `cargo vendor` to materialise every transitive dep in
#      vendor/, writes the offline-source `.cargo/config.toml` that
#      the spec/rules expect.
#   4. Builds a single `claude-sandbox-<version>.tar.gz` containing
#      the source tree + vendor/ + .cargo/config.toml. This is the
#      file OBS consumes as Source0.
#   5. Prints the OBS upload commands; running `osc` is left to the
#      maintainer (so this script is safe to invoke without `osc`
#      installed).
#
# This is offline-builds-only: the spec & debian/rules both invoke
# `cargo build --offline --frozen` and rely on vendor/ being inside
# the tarball.

set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
cd "$repo_root"

# --- 1. Clean tree check ---
if [[ -n "$(git status --porcelain)" ]]; then
  echo "error: working tree is dirty; commit or stash before releasing." >&2
  exit 1
fi

# --- 2. Version + tag check ---
version=$(grep -m1 '^version' Cargo.toml | sed -E 's/version *= *"([^"]+)"/\1/')
if [[ -z "$version" ]]; then
  echo "error: couldn't extract version from Cargo.toml" >&2
  exit 1
fi

expected_tag="v$version"
head_tags=$(git tag --points-at HEAD)
if ! echo "$head_tags" | grep -qx "$expected_tag"; then
  echo "error: HEAD is not tagged $expected_tag (got tags: ${head_tags:-none})." >&2
  echo "       Run: git tag -a $expected_tag -m \"<message>\"" >&2
  exit 1
fi

if ! git cat-file -t "$expected_tag" | grep -qx tag; then
  echo "error: $expected_tag is a lightweight tag; OBS needs annotated." >&2
  echo "       Delete and re-tag with: git tag -a $expected_tag -m \"...\"" >&2
  exit 1
fi

echo "==> Releasing claude-sandbox $version (HEAD at $expected_tag)"

# --- 3. cargo vendor ---
echo "==> cargo vendor"
rm -rf vendor .cargo/config.toml
cargo vendor > /tmp/cargo-vendor-config.toml
mkdir -p .cargo
mv /tmp/cargo-vendor-config.toml .cargo/config.toml

# --- 4. Tarball ---
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
dest="$stage/claude-sandbox-$version"
mkdir -p "$dest"

# Copy tracked files (no target/, no .git/) plus the vendor + .cargo
# bits we just generated.
git ls-files --cached --others --exclude-standard | \
  grep -v '^vendor/' | \
  grep -v '^\.cargo/' | \
  tar -cf - -T - | tar -xf - -C "$dest"
cp -r vendor "$dest/vendor"
cp -r .cargo "$dest/.cargo"

tarball="$repo_root/packaging/obs/claude-sandbox-$version.tar.gz"
tar -C "$stage" -czf "$tarball" "claude-sandbox-$version"
echo "==> Wrote $tarball"

# Cleanup the vendor/ from the working tree so it stays out of git
# (vendor is in .gitignore per the gitignore step below).
rm -rf vendor .cargo

# --- 5. Next steps ---
cat <<EOF

==> Done. Next steps to publish via OBS:

  cd ~/work/home:houtamelo/claude-sandbox       # your osc checkout
  cp $repo_root/packaging/obs/{_service,claude-sandbox.spec,debian.*} .
  cp $tarball .
  osc add claude-sandbox-$version.tar.gz _service claude-sandbox.spec debian.*
  osc commit -m "release $version"

OBS will then build .deb / .rpm for every enabled target.
Watch the project dashboard at:
  https://build.opensuse.org/package/show/home:houtamelo/claude-sandbox

EOF
