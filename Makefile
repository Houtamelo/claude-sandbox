.PHONY: build image clean

build:
	cargo build --release

# Build the in-container image after a binary build. Standalone target
# because the image bakes the freshly-built binary plus the resolved
# Dockerfile from the three-tier asset lookup; running it without `build`
# first would bake whatever `target/release/claude-sandbox` happens to
# be on disk. Run `make build image` to rebuild both in one go.
image: build
	@build_dir="$$HOME/.cache/claude-sandbox/build"; \
	rm -rf "$$build_dir"; \
	mkdir -p "$$build_dir"; \
	cp target/release/claude-sandbox "$$build_dir/claude-sandbox"; \
	cp assets/Dockerfile "$$build_dir/Dockerfile"; \
	cp assets/CLAUDE.md "$$build_dir/CLAUDE.md"; \
	podman build -t claude-sandbox:0.1 -f "$$build_dir/Dockerfile" "$$build_dir"

clean:
	cargo clean
