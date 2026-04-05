---
name: rust-coverage
description: Generate Rust code coverage reports from Python tests for this PyO3/maturin project. Use when asked about code coverage, uncovered lines, or test coverage.
---

# Rust Code Coverage (from Python tests)

Coverage uses `cargo-llvm-cov`, which handles instrumentation flags, profraw
management, and report generation.

## Quick path

```sh
make coverage        # instrumented build + pytest + summary table
```

## Per-file detail

To see uncovered lines for a specific file, run the pipeline manually:

```sh
eval "$(cargo llvm-cov show-env --sh)"
cargo llvm-cov clean --profraw-only
uv sync --reinstall-package tomledit
pytest -q
LLVM_COV_FLAGS="--show-line-counts-or-regions --sources src/item_proxy.rs" \
  cargo llvm-cov report --release
```

## Clean up (mandatory)

The instrumented `.so` stays installed until you rebuild.  Any later
`uv run` will load it and write profraw files.  Always rebuild clean:

```sh
uv sync --reinstall-package tomledit
```
