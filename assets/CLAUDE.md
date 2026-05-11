# Sandbox environment

You are running inside **claude-sandbox** — a rootless Podman container
isolated from the host. This is a private, throwaway environment;
nothing you do here can damage the host system.

## Your privileges

- You are user `claude` (UID 1000), non-root by design (Claude Code
  refuses `--dangerously-skip-permissions` as root).
- You have **passwordless sudo** for everything. `sudo apt install <pkg>`,
  `sudo systemctl ...`, editing `/etc`, etc. all just work — no prompts.
- The container is the safety boundary. Skip the usual hesitation about
  installing tools, modifying system files, or running build scripts.
  If a task needs a package, install it. If it needs a daemon, start it.

## Layout

- `/work` — the user's project (bind-mounted from the host). **Edits
  here propagate to the host.** Treat this dir as you would the user's
  real project on the host.
- `~` (your home) is bind-mounted partially: `~/.claude/` (settings,
  credentials, agents, plugins), `~/.claude.json` (onboarding/account
  state), `~/.cache/claude-cli-nodejs/`, and `~/.cache/claude/` all map
  back to host so your config and history persist.
- Everything else under `~` and the rest of the FS lives only in this
  container's named volume and writable layer — destroyed by
  `claude-sandbox down`. Persistent installs should go in the
  `setup = [...]` array of `.claude-sandbox.toml` so they're reapplied
  on container recreation.

## `cs` — in-container companion CLI

Use these from inside the container:

| Command | What it does |
|---|---|
| `cs status` | Show project path + current worktree name |
| `cs worktree add <name> [--branch <branch>]` | Create a git worktree at `/work/.worktrees/<name>`. Runs `worktree_setup` hooks from `.claude-sandbox.toml` with `$CS_WORKTREE_NAME` exported. |
| `cs worktree ls` | List worktrees with their paths and branches |
| `cs worktree rm <name>` | Remove a worktree (force + prune) |
| `cs worktree current` | Print the current worktree name (or `main`) |
| `cs apply` | Re-run `.claude-sandbox.deps.sh` (see below) without recreating the container |

If you decide you want to work in an isolated worktree (e.g. to try a
risky refactor without touching `main`), use `cs worktree add`. The
host wrapper (`claude-sandbox -w <name>`) can later attach claude
sessions to that specific worktree.

## Persisting dependencies — `.claude-sandbox.deps.sh`

When you `apt install` / `cargo install` / etc. a tool, it lives in the
container's writable layer. **That layer is destroyed by `claude-sandbox
down`**, so the install vanishes on container reset. The host user may
also recreate the container occasionally (image rebuild, reboot quirks).

To make a dependency survive resets, append the install command to
`/work/.claude-sandbox.deps.sh`. This file:

- Lives alongside the user's `.claude-sandbox.toml` and is editable by you.
- Runs as **root** on every container creation (no sudo prefix needed inside).
- Is the canonical place to record "this project needs tool X".

Workflow:

```bash
# You install something now:
sudo apt install -y ripgrep

# Persist it for future container recreations:
echo "apt install -y ripgrep" >> /work/.claude-sandbox.deps.sh

# If you want to verify the script works idempotently right now:
cs apply
```

Make commands idempotent (`apt install -y` is; `echo foo >> ~/.bashrc`
is NOT — guard with a grep first). The script is a regular `bash` script,
so you can use loops, conditionals, package-manager lists, etc.

Differs from `.claude-sandbox.toml`'s `setup = [...]` array: `setup` is
the **user's** static project config (typically committed); deps.sh is
**your** scratch space for installs you needed mid-session. Both run on
container creation; deps.sh runs after setup.

## Network

You have full outbound network. Inbound ports are not published unless
the user's `.claude-sandbox.toml` declares them.

## What's outside your reach

- The host filesystem outside the bind-mounts above. Don't try to
  navigate out of `/work` or `~` expecting to find host files; those
  paths don't exist here.
- The host's other containers, processes, or users.
- SSH/AWS/cloud credentials unless the user explicitly opted in via
  per-project `mount` or `env_passthrough` config.
- SSH agent keys: forwarded by default (the agent socket is at
  `$SSH_AUTH_SOCK`), so `git push`, `ssh <host>`, etc. work — but the
  key files themselves are not visible.
