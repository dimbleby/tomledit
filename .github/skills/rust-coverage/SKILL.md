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

Use `--lcov` output.
Build first with `make coverage-build`, then run the rest in a **subshell** so
the coverage environment does not leak into your own shell — anything run
afterwards with those variables set builds instrumented code by accident.

### All files

```sh
(
  eval "$(cargo llvm-cov show-env --sh)" &&
  cargo llvm-cov clean --profraw-only &&
  pytest -q >&2 &&
  cargo llvm-cov report --profile coverage --lcov
) | awk '
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

- `cargo llvm-cov report` gives summary tables; there is no `show` subcommand.
  Use `--lcov` for line-level data.
- The `--sources` flag does not work with this project.
- Running the steps in separate shells causes "not found \*.profraw".
- Run `make` targets from a plain shell: they set up the coverage environment
  themselves, and fail with "nested show-env" inside a shell that has already
  sourced it.
- An instrumented `.so` left installed makes *any* later Python run drop a
  `default_*.profraw` in the working directory, so clean up when you are done.

## Clean up

The instrumented `.so` stays installed until rebuilt:

```sh
make build
```
