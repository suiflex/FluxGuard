.PHONY: build fmt fmt-check lint test check npm-test npm-pack

build:
	cargo build --workspace

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

lint:
	cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
	cargo test --workspace

npm-test:
	node npm/test.js

npm-pack:
	cd npm && npm pack --dry-run --ignore-scripts

check: fmt-check lint test npm-test
