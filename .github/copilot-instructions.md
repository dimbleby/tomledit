# Copilot Instructions for tomledit

## What This Is

A Python library wrapping Rust's `toml_edit` crate via PyO3, providing
format-preserving TOML editing with a dict/list-like API.

Users interact with `Document`, `Item`, and its three subclasses `DictItem`,
`ListItem`, and `ScalarItem`.  All are live references into a shared
`toml_edit::DocumentMut`; mutations through one reference are visible to all
others that share the same document.

## Build, Test, and Lint

```sh
make build          # Build and install the package (no-op if up to date)
make test           # Rust unit tests + Python tests (builds first)
make lint           # All linters: fmt, clippy, ruff, mypy, ty
make coverage       # Instrumented build + llvm-cov report
```

To run a single test: `make build && pytest tests/test_comments.py::TestComment::test_set_comment -v`

## Architecture

### Frozen classes with `RwLock`

All `#[pyclass]` types use `#[pyclass(frozen)]`.  Mutable state lives behind
`RwLock` (genuinely `Sync`, safe under both GIL and free-threading).

- **`Document`** fields: `inner: RwLock<DocumentMut>`,
  `trie: RwLock<MutationTrie>`.  The trie owns the monotonic revision
  counter — revision and trie stamps are always updated together under one
  write lock.
- **`ItemProxy`** fields: `document: Py<Document>`, `path: Vec<Key>`,
  `revision: AtomicU64`.
- **Subclass proxies** (`DictProxy`, `ListProxy`, `ScalarProxy`) are unit
  structs — all state lives in the `ItemProxy` base class.

### Python-visible classes

- **`Document`** (`document.rs`) — wraps `toml_edit::DocumentMut`.
  `MutableMapping`.  `Document.parse(text)` round-trips; `Document()` or
  `Document({"key": "value"})` creates new.  `str(doc)` → Python repr;
  `doc.as_toml()` → TOML text.
- **`Item`** (`item_proxy.rs`) — base class for live references.
  Each `__getitem__` returns a new proxy with a longer path — navigation
  happens at call-time, so `doc["a"]["b"] = x` works without cloning.
- **`DictItem`** (`DictProxy`) — tables / inline tables.  `MutableMapping`.
- **`ListItem`** (`ListProxy`) — arrays / arrays of tables.
  `MutableSequence`, plus slice get/set/del.
- **`ScalarItem`** (`ScalarProxy`) — forwards arithmetic, comparison,
  hashing, `int()`, `float()`, `format()` to the underlying Python value.
- **`KeysView`**, **`ValuesView`**, **`ItemsView`** (`views.rs`) — live
  dictionary views.  Re-navigate on each access.

### Method patterns

`ItemProxy` methods use `&self` directly.  The `checked_doc` helper binds
the document and verifies freshness in one call:

```rust
pub fn my_method(&self, py: Python<'_>) -> PyResult<...> {
    let doc = self.checked_doc(py)?;
    let inner = doc.inner.read().unwrap();
    let item = self.navigate(&inner)?;
    ...
}
```

Subclass methods use `slf: &Bound<'_, Self>` + `slf.as_super().get()`:

```rust
pub fn my_method(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<...> {
    let base = slf.as_super().get();
    let doc = base.checked_doc(py)?;
    ...
}
```

### Staleness / proxy invalidation

The **`MutationTrie`** (`trie.rs`) owns a monotonic revision counter.
Every mutation stamps the trie at the mutated path and increments the
revision (both under one write lock).  Each proxy records the revision at
creation.  Before any access, a proxy walks the trie — if any ancestor
node has `revised_at > proxy.revision`, the proxy is **stale** and raises
`RuntimeError`.  Mutations to unrelated subtrees do **not** invalidate a
proxy (path-precise invalidation).

### Rust module layout

`lib.rs` lists all modules and registers PyO3 classes — read it first.

| File | Role |
|------|------|
| `document.rs` | `Document` class |
| `item_proxy.rs` | `ItemProxy` base + `resolve_proxy()` + `checked_doc()` helper |
| `dict_proxy.rs` / `list_proxy.rs` / `scalar_proxy.rs` | Subclass pymethods |
| `item_ops.rs` / `dict_ops.rs` / `list_ops.rs` | Pure logic helpers (no PyO3 class) |
| `item.rs` | Thin `Item(toml_edit::Item)` newtype for `FromPyObject` — **not** the Python-visible `Item` |
| `value.rs` | Python ↔ `toml_edit` type conversion |
| `equality.rs` | Semantic equality between TOML items and Python objects |
| `comments.rs` | Comment get/set; array element comments live in the _next_ element's decor prefix |
| `views.rs` | `KeysView`, `ValuesView`, `ItemsView` |
| `trie.rs` | `MutationTrie` + revision counter for path-precise staleness |

## Key Conventions

**Comment API:** `.comment` is the block comment above an entry;
`.inline_comment` is the trailing `# ...` on the same line.
Both include the `#` character — users write `"# my comment"`, not `"my
comment"`.  `None` clears a comment.

**Serialization:** `str()` returns a Python repr.  `as_toml()` returns
TOML text.

**Error types:** `PyKeyError` for missing keys, `PyIndexError` for
out-of-range indices, `PyTypeError` for wrong-type arguments, `PyValueError`
for invalid values, `PyRuntimeError` for stale proxies.

**Linting:** Ruff `select = ["ALL"]`, `strict = true` mypy, `cargo clippy`.

**Rust edition 2024** with `toml_edit 0.25` and `pyo3 0.28`.

**Tests** are in `tests/`, split by concern.  Shared fixtures in
`conftest.py`.  Use `from __future__ import annotations` (enforced by ruff).

**Type stubs** in `tomledit.pyi` must be updated when the Python API changes.

## Pitfalls

- **Forgetting `--reinstall-package tomledit`.** After touching any `.rs`
  file, `uv run pytest` runs stale code.  Use `make build` or
  `uv sync --reinstall-package tomledit`.
- **Using `str(doc)` for TOML output.** Use `doc.as_toml()`.
- **Forgetting to bump.** Every mutation must record itself in the trie.
  `bump_self` for structural changes (insert/remove/clear),
  `bump_child` for replacing a child by key.
- **Lock conflicts.** When a method receives a `value: &Bound<'_, PyAny>`
  that might be a proxy from the same document, extract it **before** taking
  `inner.write()` — otherwise the write lock can deadlock with a nested read.
  Use `resolve_proxy()` or `.extract::<Item>()` first.  See
  `ListProxy::remove` for the two-phase (read then write) pattern.
- **Forgetting `checked_doc`.** Every proxy method must verify freshness
  before navigating.  Use `self.checked_doc(py)?` or `self.check_fresh(doc)?`.
- **`from __future__ import annotations`** is required in every Python file.
