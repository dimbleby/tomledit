---
name: rust-coverage
description: Generate Rust code coverage reports from Python tests for this PyO3/maturin project. Use when asked about code coverage, uncovered lines, or test coverage.
---

# Rust Code Coverage (from Python tests)

Uses `cargo-llvm-cov` for instrumentation and report generation.

## Summary table

```sh
make coverage
```

## Uncovered line numbers

Use `--lcov` output. **All steps must run in the same shell** (shared env
vars point pytest's profraw output to the right directory).

### All files

```sh
eval "$(cargo llvm-cov show-env --sh)" && \
  cargo llvm-cov clean --profraw-only && \
  pytest -q && \
  cargo llvm-cov report --release --lcov | awk '
    /^SF:/ { file=$0; sub(/^SF:/, "", file); sub(/.*\/src\//, "src/", file) }
    /^DA:/ && /,0$/ { line=$0; sub(/^DA:/, "", line); sub(/,0$/, "", line); print file ":" line }
  '
```

### Single file

```sh
# same pipeline, filter in the awk:
  ... | awk '
    /^SF:/ { file=$0; sub(/^SF:/, "", file); sub(/.*\/src\//, "src/", file) }
    /^DA:/ && /,0$/ { line=$0; sub(/^DA:/, "", line); sub(/,0$/, "", line);
      if (file == "src/list_ops.rs") print line }
  '
```

## Pitfalls

- `cargo llvm-cov report` gives summary tables; there is no `show`
  subcommand. Use `--lcov` for line-level data.
- The `--sources` flag does not work with this project.
- Running the steps in separate shells causes "not found *.profraw".
- `make coverage-build` handles the instrumented build.

## Clean up

The instrumented `.so` stays installed until rebuilt:

```sh
uv sync --reinstall-package tomledit
```
