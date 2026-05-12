#
# spec file for package claude-sandbox
#
# Build-time deps cover both the openSUSE/Fedora rpm family. The `cargo`
# package on every supported target pulls a recent-enough `rustc` and
# `cargo` (>= 1.84 needed for edition = "2024").
#
# Runtime `Requires: podman` is hard — claude-sandbox is a podman wrapper.
# `fuse-overlayfs` is a hard dep on Tumbleweed where the default `runc` +
# kernel overlay combo fails on `--userns=keep-id`. `crun` is Recommended
# (Tumbleweed default is `runc`, switching is documented in our README).
#

Name:           claude-sandbox
Version:        0.2.0
Release:        0
Summary:        Run Claude Code in a rootless-Podman per-project sandbox
License:        MIT
URL:            https://github.com/Houtamelo/claude-sandbox
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo
BuildRequires:  rust >= 1.84
BuildRequires:  pkgconfig

Requires:       podman
Requires:       fuse-overlayfs
Recommends:     crun
Recommends:     acl

%description
claude-sandbox runs Claude Code in a rootless-Podman per-project sandbox.
Inside, Claude has full passwordless sudo and apt-install privileges;
the host system is structurally protected by Podman's user-namespace
isolation. One sandbox container per project, with auto-rebuild on
configuration changes, GPU passthrough, OAuth-token-based authentication
(avoids `.credentials.json` refresh-token collisions on shared hosts),
and a host-side KDE Dolphin "Open in claude-sandbox" context-menu entry.

%prep
%setup -q

# Vendored crates are tarballed alongside the source by
# packaging/release.sh and committed at the tagged release. Wire the
# vendor dir into cargo so --offline finds every dep locally.
mkdir -p .cargo
cat > .cargo/config.toml <<'EOF'
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor"
EOF

%build
cargo build --release --offline --frozen

%install
install -Dm755 target/release/claude-sandbox %{buildroot}%{_bindir}/claude-sandbox
install -Dm644 assets/Dockerfile %{buildroot}%{_datadir}/claude-sandbox/Dockerfile
install -Dm644 assets/default-config.toml %{buildroot}%{_datadir}/claude-sandbox/config.toml
install -Dm644 assets/CLAUDE.md %{buildroot}%{_datadir}/claude-sandbox/CLAUDE.md
install -Dm755 assets/dolphin/open-in-claude-sandbox.desktop \
    %{buildroot}%{_datadir}/kio/servicemenus/open-in-claude-sandbox.desktop

%files
%license LICENSE
%doc README.md
%{_bindir}/claude-sandbox
%dir %{_datadir}/claude-sandbox
%{_datadir}/claude-sandbox/Dockerfile
%{_datadir}/claude-sandbox/config.toml
%{_datadir}/claude-sandbox/CLAUDE.md
%{_datadir}/kio/servicemenus/open-in-claude-sandbox.desktop

%changelog
* Tue May 12 2026 Houtamelo <houtamelo@users.noreply.github.com> - 0.2.0-0
- First OBS-packaged release.
- FHS layout: binary in /usr/bin, assets in /usr/share/claude-sandbox,
  KDE servicemenu in /usr/share/kio/servicemenus.
- Three-tier runtime asset lookup (~/.config -> /usr/share -> embedded)
  so users can edit Dockerfile/config.toml without breaking the package.
