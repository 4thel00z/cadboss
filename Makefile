.DEFAULT_GOAL := help
.PHONY: build release fmt fmt-check lint test ci develop test-py wheel doc bench samples fuzz help

##@ Build

build: ## Build the workspace in debug mode
	cargo build --workspace

release: ## Build the workspace in release mode
	cargo build --workspace --release

##@ Quality

fmt: ## Format all Rust code
	cargo fmt --all

fmt-check: ## Check Rust formatting
	cargo fmt --all -- --check

lint: ## Run clippy with warnings denied
	cargo clippy --workspace --all-targets -- -D warnings

test: ## Run the Rust test suite
	cargo test --workspace

doc: ## Build API docs with warnings denied
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

ci: fmt-check lint test doc ## Run every check CI runs

##@ Python

develop: ## Install the Python extension into the uv environment
	uv run maturin develop

test-py: ## Run the Python test suite
	uv run pytest tests -q

wheel: ## Build a release wheel
	uv run maturin build --release

##@ Extras

bench: ## Run benchmarks
	cargo bench --workspace

samples: ## Fetch sample drawings (not yet implemented)
	@echo "not yet"

fuzz: ## Run fuzz targets (not yet implemented)
	@echo "not yet"

##@ Help

help: ## Show this help
	@awk 'BEGIN {FS = ":.*##"; printf "Usage: make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*##/ {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2} /^##@/ {printf "\n\033[1m%s\033[0m\n", substr($$0, 5)}' $(MAKEFILE_LIST)
