# Copilot Instructions for tomledit

## What This Is

A Python library wrapping Rust's `toml_edit` crate via PyO3, providing
format-preserving TOML editing with a dict/list-like API.
Users interact with `Document` and `Item` (internally `ItemProxy`) classes.

## Build, Test, and Lint

```sh
# Build and install (requires Rust toolchain + maturin)
uv sync

# Run full test suite
uv run pytest

# Run a single test
uv run pytest tests/test_comments.py::TestComment::test_set_comment -v

# Run a test class
uv run pytest tests/test_comments.py::TestComment -v

# Rust lint and format
cargo fmt
cargo clippy --all-targets -- -D warnings

# Python lint and format
ruff check .
ruff format .
```

After changing Rust code, rebuild with `uv run --reinstall-package tomledit
pytest` to pick up changes.

### Rust code coverage (from Python tests)

`cargo-llvm-cov`'s wrapper doesn't work with maturin, so use the manual
approach:

```sh
# Build instrumented .so and run tests (generates .profraw)
rm -f target/tomledit-*.profraw
RUSTFLAGS="-Cinstrument-coverage" \
  LLVM_PROFILE_FILE="target/tomledit-%p-%m.profraw" \
  uv run --reinstall-package tomledit pytest -q

# Merge profiles and generate report
LLVM_TOOLS_PATH="$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin"
"$LLVM_TOOLS_PATH/llvm-profdata" merge -sparse target/tomledit-*.profraw -o target/tomledit.profdata
"$LLVM_TOOLS_PATH/llvm-cov" report \
  target/x86_64-unknown-linux-gnu/release/libtomledit.so \
  --instr-profile=target/tomledit.profdata --sources src/

# Per-file detail (uncovered lines)
"$LLVM_TOOLS_PATH/llvm-cov" show \
  target/x86_64-unknown-linux-gnu/release/libtomledit.so \
  --instr-profile=target/tomledit.profdata \
  --sources src/item_proxy.rs --show-line-counts-or-regions

# IMPORTANT: rebuild without instrumentation when done, otherwise
# every subsequent `uv run` will produce .profraw files.
uv sync --reinstall-package tomledit
```

## Architecture

**Rust → Python boundary** uses two PyO3 classes:

- **`Document`** (`document.rs`): Wraps `toml_edit::DocumentMut`.
  Dict-like interface for top-level keys.
  `parse(text)` is the entry point.
- **`ItemProxy`** (`item_proxy.rs`): Exported as `Item` in Python.
  Holds `Py<Document>` + `Vec<Key>` path.
  Each `__getitem__` returns a new proxy with a longer path \- mutations navigate
  the path at call-time into the shared document, so `doc["a"]["b"] = x` works
  without cloning.

**Supporting modules:**

- `comments.rs` \- Comment get/set logic.
  Inline comments live in decor suffix; block comments in decor prefix.
  Array element comments are stored in the _next_ element's prefix (or array
  trailing for the last).
- `equality.rs` \- Structural equality between toml_edit items and Python
  objects.
- `value.rs` \- Python → toml_edit type conversion (extracts dicts, lists,
  datetimes, scalars).
- `item.rs` \- Thin `Item` wrapper for PyO3 `FromPyObject`.

## Key Conventions

**Comment API:** `.comment` is the block comment above an entry;
`.inline_comment` is the trailing `# ...` on the same line.
Both include the `#` character \- users write `"# my comment"`, not `"my
comment"`.
Non-empty lines in block comments must start with `#`; empty lines represent
blank lines.
`None` clears a comment.

**Rust edition 2024** with `toml_edit 0.25` and `pyo3 0.28`.

**Tests** are in `tests/`, split by concern: `test_read_write.py`,
`test_document.py`, `test_proxy.py`, `test_equality.py`, `test_comments.py`,
`test_errors.py`.
Shared fixtures live in `conftest.py`.
Use `from __future__ import annotations` (enforced by ruff).

**Type stubs** in `tomledit.pyi` must be updated when the Python API changes.
