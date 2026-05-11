PREFIX ?= $(HOME)/.local
CONFIG_DIR := $(HOME)/.config/claude-sandbox

.PHONY: build install image clean

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

clean:
	cargo clean
