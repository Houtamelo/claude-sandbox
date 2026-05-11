PREFIX ?= $(HOME)/.local
CONFIG_DIR := $(HOME)/.config/claude-sandbox
KDE_SERVICEMENU_DIR := $(HOME)/.local/share/kio/servicemenus

.PHONY: build install image install-dolphin-menu uninstall-dolphin-menu clean

build:
	cargo build --release

install: build
	install -Dm755 target/release/claude-sandbox $(PREFIX)/bin/claude-sandbox
	install -d $(CONFIG_DIR)
	[ -f $(CONFIG_DIR)/Dockerfile ] || install -m644 assets/Dockerfile $(CONFIG_DIR)/Dockerfile
	[ -f $(CONFIG_DIR)/config.toml ] || install -m644 assets/default-config.toml $(CONFIG_DIR)/config.toml
	@echo "installed to $(PREFIX)/bin/claude-sandbox"

image:
	cp target/release/claude-sandbox $(CONFIG_DIR)/claude-sandbox
	podman build -t claude-sandbox:0.1 -f $(CONFIG_DIR)/Dockerfile $(CONFIG_DIR)
	rm $(CONFIG_DIR)/claude-sandbox

# KDE/Dolphin context-menu entry. After install, right-click in any
# folder (or on a folder) in Dolphin shows "Open in claude-sandbox".
# KF6 requires the .desktop to be executable (mode 755).
install-dolphin-menu:
	install -d $(KDE_SERVICEMENU_DIR)
	install -m755 assets/dolphin/open-in-claude-sandbox.desktop \
		$(KDE_SERVICEMENU_DIR)/open-in-claude-sandbox.desktop
	@echo "installed Dolphin servicemenu. Right-click a folder to test."
	@echo "(If konsole isn't your terminal, edit the Exec line in:"
	@echo "  $(KDE_SERVICEMENU_DIR)/open-in-claude-sandbox.desktop)"

uninstall-dolphin-menu:
	rm -f $(KDE_SERVICEMENU_DIR)/open-in-claude-sandbox.desktop
	@echo "uninstalled."

clean:
	cargo clean
