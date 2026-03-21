---
name: rust-coverage
description: Generate Rust code coverage reports from Python tests for this PyO3/maturin project. Use when asked about code coverage, uncovered lines, or test coverage.
---

# Rust Code Coverage (from Python tests)

`cargo-llvm-cov`'s wrapper doesn't work with maturin, so use the manual
approach.

## Step 1 — Build instrumented `.so` and run tests

**Every command that touches the instrumented `.so` must have
`LLVM_PROFILE_FILE` set**, otherwise LLVM writes `default_*.profraw` files
into the repo root. Pass it inline on every command rather than relying on
`export` (which is lost if the shell session changes).

```sh
# Clean previous profiles
rm -f target/tomledit-*.profraw

# Build + test in one command (LLVM_PROFILE_FILE is set inline)
LLVM_PROFILE_FILE="target/tomledit-%p-%m.profraw" \
  RUSTFLAGS="-Cinstrument-coverage" \
  uv run --reinstall-package tomledit pytest -q
```

## Step 2 — Generate reports

```sh
HOST_TRIPLE="$(rustc -vV | sed -n 's/host: //p')"
LLVM_TOOLS_PATH="$(rustc --print sysroot)/lib/rustlib/${HOST_TRIPLE}/bin"

# Merge profiles
"$LLVM_TOOLS_PATH/llvm-profdata" merge -sparse \
  target/tomledit-*.profraw -o target/tomledit.profdata

# Summary table
"$LLVM_TOOLS_PATH/llvm-cov" report \
  target/${HOST_TRIPLE}/release/libtomledit.so \
  --instr-profile=target/tomledit.profdata --sources src/

# Per-file detail (uncovered lines)
"$LLVM_TOOLS_PATH/llvm-cov" show \
  target/${HOST_TRIPLE}/release/libtomledit.so \
  --instr-profile=target/tomledit.profdata \
  --sources src/item_proxy.rs --show-line-counts-or-regions
```

## Step 3 — Clean up (mandatory)

The instrumented `.so` stays installed until you rebuild.  **Any** later
`uv run` will load it and write profraw files — with `default_*` names if
`LLVM_PROFILE_FILE` is unset.  Always rebuild clean before finishing:

```sh
uv sync --reinstall-package tomledit
rm -f target/tomledit-*.profraw target/tomledit.profdata
```
