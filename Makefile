.PHONY: help build test test-rust test-swift lint fmt clean

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

build: ## Rebuild budget_coreFFI.xcframework + regenerate Swift bindings
	scripts/build-xcframework.sh

test: test-rust test-swift ## Run the full test suite (Rust + Swift/FFI)

test-rust: ## Rust unit tests (solver logic)
	cargo test

test-swift: ## Swift tests through the UniFFI boundary (uses committed xcframework)
	swift test

lint: ## Clippy with warnings as errors + rustfmt check
	cargo clippy --all-targets --features cli -- -D warnings
	cargo fmt --check

fmt: ## Format Rust sources
	cargo fmt

clean: ## Remove Rust and SwiftPM build products
	cargo clean
	rm -rf .build generated
