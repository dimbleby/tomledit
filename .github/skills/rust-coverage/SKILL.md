---
name: rust-coverage
description: Generate Rust code coverage reports from Python tests for this PyO3/maturin project. Use when asked about code coverage, uncovered lines, or test coverage.
---

# Rust Code Coverage (from Python tests)

`cargo-llvm-cov`'s wrapper doesn't work with maturin, so use the manual
approach. **Always export `LLVM_PROFILE_FILE` first** — this ensures every
process that loads the instrumented `.so` writes `.profraw` files into
`target/` rather than the repo root.

```sh
# Set for the entire shell session — do this first!
export LLVM_PROFILE_FILE="target/tomledit-%p-%m.profraw"

# Build instrumented .so and run tests (generates .profraw in target/)
rm -f target/tomledit-*.profraw
RUSTFLAGS="-Cinstrument-coverage" \
  uv run --reinstall-package tomledit pytest -q

# Merge profiles and generate report
HOST_TRIPLE="$(rustc -vV | sed -n 's/host: //p')"
LLVM_TOOLS_PATH="$(rustc --print sysroot)/lib/rustlib/${HOST_TRIPLE}/bin"
"$LLVM_TOOLS_PATH/llvm-profdata" merge -sparse target/tomledit-*.profraw -o target/tomledit.profdata
"$LLVM_TOOLS_PATH/llvm-cov" report \
  target/${HOST_TRIPLE}/release/libtomledit.so \
  --instr-profile=target/tomledit.profdata --sources src/

# Per-file detail (uncovered lines)
"$LLVM_TOOLS_PATH/llvm-cov" show \
  target/${HOST_TRIPLE}/release/libtomledit.so \
  --instr-profile=target/tomledit.profdata \
  --sources src/item_proxy.rs --show-line-counts-or-regions
```

**IMPORTANT:** Rebuild without instrumentation when done, otherwise every
subsequent `uv run` will produce `.profraw` files:

```sh
uv sync --reinstall-package tomledit
rm -f target/tomledit-*.profraw
unset LLVM_PROFILE_FILE
```
