# claude-sandbox

Run Claude Code in a rootless-Podman per-project sandbox.
Claude gets full `sudo` inside; your host is unaffected.

## Install

    make install     # builds and installs ~/.local/bin/claude-sandbox
    claude-sandbox init      # writes ~/.config/claude-sandbox/{Dockerfile,config.toml}
    claude-sandbox rebuild   # builds the base image (claude-sandbox:0.1)

## Use

    cd ~/some-project
    claude-sandbox             # creates a container on first run, launches `claude`
    claude-sandbox shell       # bash inside
    claude-sandbox stop        # preserves state
    claude-sandbox down        # destroys container + named home volume
    claude-sandbox ls          # list all cs-* containers

See [docs/2026-05-10-design.md](docs/2026-05-10-design.md) for the full design.
