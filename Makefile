.PHONY: help lint test fuzz fmt clippy rust-test pytest ruff-check ruff-format mypy ty coverage-build coverage clean

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

lint: fmt clippy ruff-check ruff-format mypy ty ## All linters: fmt, clippy, ruff, mypy, ty

test: rust-test pytest ## Rust + Python tests

fmt:
	cargo fmt --check

clippy:
	cargo clippy --all-targets -- -D warnings

rust-test:
	LD_LIBRARY_PATH="$$(python3 -c 'import sysconfig; print(sysconfig.get_config_var("LIBDIR"))')" cargo test

RUST_SOURCES := $(shell find src -name '*.rs')

build: .build-stamp ## Build and install (no-op if up to date)
.build-stamp: $(RUST_SOURCES) Cargo.toml Cargo.lock pyproject.toml uv.lock
	uv sync --reinstall-package tomledit
	@rm -f .coverage-stamp
	@touch $@

pytest: build
	pytest

fuzz: build ## Hypothesis property tests (slow)
	pytest -m slow -v

ruff-check:
	ruff check .

ruff-format:
	ruff format --check .

mypy:
	mypy

ty:
	ty check

coverage-build: .coverage-stamp
.coverage-stamp: $(RUST_SOURCES) Cargo.toml Cargo.lock pyproject.toml uv.lock
	eval "$$(cargo llvm-cov show-env --sh)" && \
		uv sync --reinstall-package tomledit
	@rm -f .build-stamp
	@touch $@

coverage: coverage-build ## Instrumented build + coverage report
	eval "$$(cargo llvm-cov show-env --sh)" && \
		cargo llvm-cov clean --profraw-only && \
		pytest -q && \
		LLVM_COV_FLAGS="--show-branch-summary=false" cargo llvm-cov report --release

clean: ## Remove build artifacts and caches
	cargo clean
	rm -f .build-stamp .coverage-stamp
	rm -rf .hypothesis .mypy_cache .pytest_cache .ruff_cache
