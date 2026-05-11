FROM debian:trixie-slim

ENV DEBIAN_FRONTEND=noninteractive
# /root/.local/bin holds the Claude Code installer's binary; must be on PATH
# so `claude` is callable directly and so our `claude-sandbox start` can exec it.
ENV PATH=/root/.local/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl git sudo bash openssh-client \
      build-essential pkg-config jq direnv \
    && rm -rf /var/lib/apt/lists/*

# Tailscale (from official repo). Trixie's apt sources use the same bookworm
# tailscale repo — they ship the same packages and are signed identically.
RUN curl -fsSL https://pkgs.tailscale.com/stable/debian/trixie.noarmor.gpg \
      -o /usr/share/keyrings/tailscale-archive-keyring.gpg \
 && curl -fsSL https://pkgs.tailscale.com/stable/debian/trixie.tailscale-keyring.list \
      -o /etc/apt/sources.list.d/tailscale.list \
 && apt-get update && apt-get install -y --no-install-recommends tailscale \
 && rm -rf /var/lib/apt/lists/*

# Claude Code (uses Anthropic's installer). Installs to /root/.local/bin/claude.
RUN curl -fsSL https://claude.ai/install.sh | bash || true

# Our binary; supplied at image build time alongside the Dockerfile.
COPY claude-sandbox /usr/local/bin/claude-sandbox
RUN chmod +x /usr/local/bin/claude-sandbox \
 && ln -sf claude-sandbox /usr/local/bin/cs

WORKDIR /work

# CMD (not ENTRYPOINT) so callers can append commands cleanly:
#   `podman create ... claude-sandbox:0.1 sleep infinity` works,
#   `podman run claude-sandbox:0.1 whoami` works,
#   bare `podman run claude-sandbox:0.1` falls back to an interactive bash.
CMD ["/bin/bash", "-l"]
