# Recipe: right-click "Open in claude-sandbox" context-menu entries

There's no portable cross-DE API for file-manager context menus on
Linux — every desktop environment uses a different mechanism, and we
only ship an auto-installer for KDE Plasma. The other DEs are easy
enough to wire up by hand; this file collects the recipes.

## KDE Plasma (Dolphin)

Auto-installed via `claude-sandbox cfg`'s desktop step (or manually
via `make install-dolphin-menu`). The artefact is a KIO servicemenu
.desktop file at `~/.local/share/kio/servicemenus/open-in-claude-sandbox.desktop`,
mode 755 (KF6 requires executable bit on servicemenus).

Right-click in any folder background or on a folder → "Open in
claude-sandbox". Launches a konsole window in that directory and runs
`claude-sandbox`.

### Changing the terminal

The shipped entry hardcodes `konsole`. To use a different terminal,
edit the `Exec=` line:

```ini
# Default (konsole):
Exec=konsole --workdir %f -e bash -c "claude-sandbox; echo; exec bash"

# Examples for other terminals:
Exec=alacritty --working-directory %f -e bash -c "claude-sandbox; echo; exec bash"
Exec=kitty --directory %f bash -c "claude-sandbox; echo; exec bash"
Exec=xterm -e bash -c "cd %f && claude-sandbox; echo; exec bash"
```

The trailing `; echo; exec bash` keeps a shell open after `claude-sandbox`
exits so the window doesn't disappear immediately on session end.

### Removing

```bash
rm ~/.local/share/kio/servicemenus/open-in-claude-sandbox.desktop
# Or, if it was installed via the Makefile:
make uninstall-dolphin-menu
```

## GNOME (Nautilus)

Nautilus uses **scripts** placed in `~/.local/share/nautilus/scripts/`.
Each script becomes a "Scripts → \<name\>" submenu entry when you
right-click a folder.

```bash
mkdir -p ~/.local/share/nautilus/scripts
cat > ~/.local/share/nautilus/scripts/Open\ in\ claude-sandbox <<'EOF'
#!/usr/bin/env bash
# Nautilus passes the selected dir as $NAUTILUS_SCRIPT_SELECTED_FILE_PATHS
# (newline-separated) or, when invoked from a folder background, sets
# $NAUTILUS_SCRIPT_CURRENT_URI to the folder being browsed.
DIR="${NAUTILUS_SCRIPT_SELECTED_FILE_PATHS:-${PWD}}"
DIR="${DIR%%$'\n'*}"  # first selected
gnome-terminal --working-directory="$DIR" -- bash -c "claude-sandbox; echo; exec bash"
EOF
chmod +x ~/.local/share/nautilus/scripts/Open\ in\ claude-sandbox
```

Right-click in Nautilus → "Scripts" → "Open in claude-sandbox".
Substitute `gnome-terminal` for whatever you use.

## XFCE (Thunar)

Thunar uses **custom actions** in `~/.config/Thunar/uca.xml`. Easier
to configure interactively: Thunar menu → Edit → Configure custom
actions → "+". Fill in:

- **Name**: Open in claude-sandbox
- **Command**: `xfce4-terminal --working-directory %f -e "bash -c 'claude-sandbox; echo; exec bash'"`
- **Appearance Conditions** tab → check "Directories"

Substitute `xfce4-terminal` for your terminal.

## Cinnamon (Nemo)

Nemo uses actions in `~/.local/share/nemo/actions/<name>.nemo_action`.
Example file:

```ini
# ~/.local/share/nemo/actions/claude-sandbox.nemo_action
[Nemo Action]
Name=Open in claude-sandbox
Comment=Launch a claude-sandbox session here
Exec=gnome-terminal --working-directory=%P -- bash -c "claude-sandbox; echo; exec bash"
Icon-Name=utilities-terminal
Selection=none
Extensions=dir
```

## MATE (Caja)

Caja is a Nautilus fork; the same scripts directory pattern works at
`~/.config/caja/scripts/` (with the corresponding `$CAJA_SCRIPT_*`
env vars).

## Other / minimal WMs

If you're on i3, sway, dwm, or any setup without a file manager
context-menu, the simplest equivalent is a shell alias or a desktop
keybinding that invokes `claude-sandbox` after `cd`'ing to a
directory you pass in. Example bash alias:

```bash
csbx() { (cd "${1:-.}" && claude-sandbox); }
```

Then `csbx ~/Documents/projects/foo` from any terminal.
