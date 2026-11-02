# Heavy local checks share machine-wide slots with other checkouts.
# CI sets CI and runs cargo directly, so it does not need this file.

.PHONY: test clippy build

test: ## Unit and integration tests
	heavy -- cargo test

clippy: ## Clippy, warnings denied
	heavy -- cargo clippy --all-targets -- -D warnings

build: ## Release binary
	heavy -- cargo build --release
