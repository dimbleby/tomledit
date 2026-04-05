.PHONY: help lint test fuzz fmt clippy rust-test pytest ruff-check ruff-format mypy ty coverage-build coverage clean

help:
	@echo "make build          Build and install (no-op if up to date)"
	@echo "make test           Rust + Python tests (builds first)"
	@echo "make fuzz           Hypothesis property tests (slow)"
	@echo "make lint           All linters: fmt, clippy, ruff, mypy, ty"
	@echo "make coverage       Instrumented build + coverage report"
	@echo "make clean          Remove build artifacts and caches"

lint: fmt clippy ruff-check ruff-format mypy ty

test: rust-test pytest

fmt:
	cargo fmt --check

clippy:
	cargo clippy --all-targets -- -D warnings

rust-test:
	LD_LIBRARY_PATH="$$(python3 -c 'import sysconfig; print(sysconfig.get_config_var("LIBDIR"))')" cargo test

RUST_SOURCES := $(shell find src -name '*.rs')

build: .build-stamp
.build-stamp: $(RUST_SOURCES) Cargo.toml Cargo.lock pyproject.toml uv.lock
	uv sync --reinstall-package tomledit
	@rm -f .coverage-stamp
	@touch $@

pytest: build
	pytest

fuzz: build
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

coverage: coverage-build
	eval "$$(cargo llvm-cov show-env --sh)" && \
		cargo llvm-cov clean --profraw-only && \
		pytest -q && \
		LLVM_COV_FLAGS="--show-branch-summary=false" cargo llvm-cov report --release

clean:
	cargo clean
	rm -f .build-stamp .coverage-stamp
	rm -rf .hypothesis .mypy_cache .pytest_cache .ruff_cache
