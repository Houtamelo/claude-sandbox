FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git sudo bash openssh-client \
      build-essential pkg-config jq direnv acl \
    && rm -rf /var/lib/apt/lists/*

# Tailscale (from official repo).
RUN curl -fsSL https://pkgs.tailscale.com/stable/debian/trixie.noarmor.gpg \
      -o /usr/share/keyrings/tailscale-archive-keyring.gpg \
 && curl -fsSL https://pkgs.tailscale.com/stable/debian/trixie.tailscale-keyring.list \
      -o /etc/apt/sources.list.d/tailscale.list \
 && apt-get update && apt-get install -y --no-install-recommends tailscale \
 && rm -rf /var/lib/apt/lists/*

# Non-root user that Claude actually runs as. Claude refuses to honour
# --dangerously-skip-permissions for any UID 0 process; the sandbox model
# requires a non-root agent inside the container. Primary group = 0 (root)
# so files Claude creates are group-readable by container root, which
# rootless podman's default userns maps to the host user — giving the
# host user group-rw on Claude-created files via umask 002.
RUN useradd -m -u 1000 -g 0 -G sudo -s /bin/bash claude \
 && echo "claude ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/claude \
 && chmod 0440 /etc/sudoers.d/claude \
 && echo "umask 002" >> /home/claude/.bashrc

# Bind-mounted project dir at /work is owned by container UID 0 (host user
# via userns), but the agent runs as `claude` (UID 1000). Git refuses to
# operate on a repo owned by a different user — disable the dubious-ownership
# check for /work specifically. ACLs grant write; this just stops the check.
RUN git config --system --add safe.directory /work \
 && git config --system --add safe.directory '/work/.worktrees/*'

# Our binary at the canonical location for any user.
COPY claude-sandbox /usr/local/bin/claude-sandbox
RUN chmod +x /usr/local/bin/claude-sandbox \
 && ln -sf claude-sandbox /usr/local/bin/cs

# Install Claude Code AS the claude user so the binary lives at
# /home/claude/.local/bin/claude (not /root/.local/bin). PATH must
# include that location for non-login shells.
USER claude
ENV HOME=/home/claude
ENV PATH=/home/claude/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
RUN curl -fsSL https://claude.ai/install.sh | bash || true

WORKDIR /work

# CMD (not ENTRYPOINT) so callers can append commands cleanly.
CMD ["/bin/bash", "-l"]
