.PHONY: help
help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-15s\033[0m %s\n", $$1, $$2}'

.PHONY: lint
lint: fmt clippy ruff-check ruff-format mypy ty ## All linters: fmt, clippy, ruff, mypy, ty

.PHONY: test
test: rust-test pytest ## Rust + Python tests

.PHONY: fmt
fmt:
	cargo fmt --check

.PHONY: clippy
clippy:
	cargo clippy --all-targets --all-features -- -D warnings

.PHONY: rust-test
rust-test:
	LD_LIBRARY_PATH="$$(python3 -c 'import sysconfig; print(sysconfig.get_config_var("LIBDIR"))')" cargo test --all-features

RUST_SOURCES := $(shell find src -name '*.rs')

.PHONY: build
build: .build-stamp ## Build and install, dev profile (no-op if up to date)
.build-stamp: $(RUST_SOURCES) Cargo.toml Cargo.lock pyproject.toml uv.lock
	MATURIN_PEP517_ARGS="--profile dev" uv sync --reinstall-package tomledit
	@rm -f .coverage-stamp
	@touch $@

.PHONY: pytest
pytest: build
	pytest

.PHONY: fuzz
fuzz: build ## Hypothesis property tests (slow)
	pytest -m slow -v

.PHONY: ruff-check
ruff-check:
	ruff check .

.PHONY: ruff-format
ruff-format:
	ruff format --check .

.PHONY: mypy
mypy:
	mypy

.PHONY: ty
ty:
	ty check

.PHONY: coverage-build
coverage-build: .coverage-stamp
.coverage-stamp: $(RUST_SOURCES) Cargo.toml Cargo.lock pyproject.toml uv.lock
	eval "$$(cargo llvm-cov show-env --sh)" && \
		MATURIN_PEP517_ARGS="--profile coverage" uv sync --reinstall-package tomledit
	@rm -f .build-stamp
	@touch $@

.PHONY: coverage
coverage: coverage-build ## Instrumented build + coverage report
	eval "$$(cargo llvm-cov show-env --sh)" && \
		cargo llvm-cov clean --profraw-only && \
		pytest -q && \
		LLVM_COV_FLAGS="--show-branch-summary=false" cargo llvm-cov report --profile coverage

.PHONY: clean
clean: ## Remove build artifacts and caches
	cargo clean
	rm -f .build-stamp .coverage-stamp
	rm -rf .hypothesis .mypy_cache .pytest_cache .ruff_cache
