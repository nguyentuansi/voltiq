# voltiq build orchestration.
# `just` is not assumed to be installed; this Makefile is the canonical task runner.
CARGO_MANIFEST := crates/Cargo.toml

.PHONY: build build-dashboard build-rust test fmt clippy clean

# Full build: dashboard static assets first, then the Rust binary that embeds them.
build: build-dashboard build-rust

build-dashboard:
	bun run --filter @voltiq/dashboard build

build-rust:
	cargo build --release --manifest-path $(CARGO_MANIFEST)

test:
	cargo test --manifest-path $(CARGO_MANIFEST)

fmt:
	cargo fmt --manifest-path $(CARGO_MANIFEST)

clippy:
	cargo clippy --manifest-path $(CARGO_MANIFEST) --all-targets

clean:
	cargo clean --manifest-path $(CARGO_MANIFEST)
	rm -rf node_modules apps/*/build apps/*/.svelte-kit .turbo
