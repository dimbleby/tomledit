# Copilot Instructions for tomledit

## What This Is

A Python library wrapping Rust's `toml_edit` crate via PyO3, providing
format-preserving TOML editing with a dict/list-like API.

Users interact with `Document`, `Item`, and its three subclasses `DictItem`,
`ListItem`, and `ScalarItem`.  All are live references into a shared
`toml_edit::DocumentMut`; mutations through one reference are visible to all
others that share the same document.

## Build, Test, and Lint

A `Makefile` provides the standard entry points:

```sh
make build          # Build and install the package (no-op if up to date)
make test           # Rust unit tests + Python tests (builds first)
make lint           # All linters: fmt, clippy, ruff, mypy, ty
make coverage       # Instrumented build + llvm-cov report
make clean          # Remove build artifacts and caches
```

Individual targets: `fmt`, `clippy`, `rust-test`, `pytest`, `ruff-check`,
`ruff-format`, `mypy`, `ty`, `coverage-build`.

To run a single test after building:

```sh
make build
pytest tests/test_comments.py::TestComment::test_set_comment -v
```

### CI

The `linting.yml` workflow runs on every push/PR to `main`:

- **Python job** — `uv sync`, `ruff check`, `ruff format --check`, `pytest`,
  `mypy` across Python 3.10–3.15.
- **Rust job** — `cargo fmt --check`, `cargo clippy`, `cargo test`.
- **Coverage job** — instrumented build + `llvm-cov` report.

## Architecture

### Python-visible classes

- **`Document`** (`document.rs`) — wraps `toml_edit::DocumentMut`.
  Registered as `collections.abc.MutableMapping`.
  `Document.parse(text)` is the entry point for round-tripping; `Document()`
  or `Document({"key": "value"})` creates a new document.
  `str(doc)` returns a dict-like repr; use `doc.as_toml()` for TOML
  serialization. `.value` returns the entire document as a plain Python dict.
- **`Item`** (`item_proxy.rs`, exported as `ItemProxy`) — base class for live
  references. Holds `Py<Document>` + `Vec<Key>` path + a creation revision.
  `Key` is an enum: `Key::Str(String)` for table keys, `Key::Int(usize)` for
  array indices (defined in `item_ops.rs`).
  Each `__getitem__` returns a new proxy with a longer path — mutations
  navigate the path at call-time, so `doc["a"]["b"] = x` works without
  cloning.
- **`DictItem`** (`DictProxy`, extends `Item`) — TOML tables and inline
  tables. Registered as `collections.abc.MutableMapping`. Provides `.keys()`,
  `.values()`, `.items()`, `.get()`, `.setdefault()`, `.pop()`, `.update()`,
  `.clear()`.
- **`ListItem`** (`ListProxy`, extends `Item`) — TOML arrays and arrays of
  tables. Registered as `collections.abc.MutableSequence`. Provides
  `.append()`, `.insert()`, `.extend()`, `.remove()`, `.count()`, `.index()`,
  `.clear()`, `.set_multiline()`, plus slice get/set/del.
- **`ScalarItem`** (`ScalarProxy`, extends `Item`) — plain values. Forwards
  arithmetic, comparison, hashing, `int()`, `float()`, `format()` to the
  underlying Python value.
- **`KeysView`**, **`ValuesView`**, **`ItemsView`** (`views.rs`) — live
  dictionary views for `Document` and `DictItem`. Registered as their
  `collections.abc` counterparts. Re-navigate the document on each access so
  they always reflect the current state. `KeysView` supports set operations
  (`&`, `|`, `-`, `^`).

### Staleness / proxy invalidation

`Document` maintains a monotonic `revision` counter and a **`MutationTrie`**
(`trie.rs`).  Every mutation stamps the trie at the mutated path and bumps
the revision.  Each proxy records the revision at creation.  Before any
access, a proxy walks the trie from root along its path — if any ancestor
node has `revised_at > proxy.revision`, the proxy is **stale** and raises
`RuntimeError`.  Mutations to unrelated subtrees do **not** invalidate a
proxy (path-precise invalidation).

### Rust module layout

`lib.rs` lists all modules and registers PyO3 classes — read it first to
orient.  The split follows a pattern:

| File | Role |
|------|------|
| `document.rs` | `Document` class |
| `item_proxy.rs` | `ItemProxy` base + `resolve_proxy()` helper |
| `dict_proxy.rs` / `list_proxy.rs` / `scalar_proxy.rs` | Subclass pymethods |
| `item_ops.rs` / `dict_ops.rs` / `list_ops.rs` | Pure logic helpers (no PyO3 class) |
| `item.rs` | Thin `Item(toml_edit::Item)` newtype for `FromPyObject` — **not** the Python-visible `Item` |
| `value.rs` | Python ↔ `toml_edit` type conversion |
| `equality.rs` | Semantic equality between TOML items and Python objects |
| `comments.rs` | Comment get/set; array element comments live in the _next_ element's decor prefix |
| `views.rs` | `KeysView`, `ValuesView`, `ItemsView` |
| `trie.rs` | `MutationTrie` for path-precise staleness |

## Key Conventions

**Comment API:** `.comment` is the block comment above an entry;
`.inline_comment` is the trailing `# ...` on the same line.
Both include the `#` character — users write `"# my comment"`, not `"my
comment"`.
Non-empty lines in block comments must start with `#`; empty lines represent
blank lines.
`None` clears a comment.

**Serialization:** `str(doc)` / `str(item)` returns a Python repr (dict-like
for tables, list-like for arrays, the value itself for scalars).
`doc.as_toml()` / `item.as_toml()` returns the TOML text.

**Error types:** use `PyKeyError` for missing keys, `PyIndexError` for
out-of-range indices, `PyTypeError` for wrong-type arguments, `PyValueError`
for invalid values (e.g. bad TOML parse), `PyRuntimeError` for stale proxies.

**Ruff config:** `select = ["ALL"]` with a short ignore list — nearly every
lint rule is active.  `ruff check .` and `ruff format .` must pass.

**mypy** is configured with `strict = true` — all Python code needs full
type annotations.

**Rust edition 2024** with `toml_edit 0.25` and `pyo3 0.28`.

**Tests** are in `tests/`, split by concern:

| File | Focus |
|------|-------|
| `test_document.py` | `Document` construction, `parse()`, `as_toml()`, copy, `value` |
| `test_proxy.py` | `Item` base-class behaviour, type narrowing |
| `test_proxy_dict.py` | `DictItem` / `MutableMapping` protocol |
| `test_proxy_list.py` | `ListItem` / `MutableSequence` protocol, slicing |
| `test_scalar_forwarding.py` | `ScalarItem` arithmetic, comparison, hashing, format |
| `test_mutations.py` | writing/mutating values, keys, arrays, tables |
| `test_equality.py` | `__eq__` between documents, items, and Python objects |
| `test_comments.py` | `.comment` / `.inline_comment` get/set |
| `test_staleness.py` | path-precise proxy invalidation |
| `test_errors.py` | expected error cases |

Shared fixtures live in `conftest.py` (`doc` fixture from `SAMPLE` TOML,
plus the `toml_literal` helper for comparing `as_toml()` output).
Use `from __future__ import annotations` (enforced by ruff isort
`required-imports`).

**Type stubs** in `tomledit.pyi` must be updated when the Python API changes.

## Pitfalls

Things agents (and humans) get wrong:

- **Forgetting `--reinstall-package tomledit`.** The single most common
  failure.  After touching any `.rs` file, `uv run pytest` runs stale code.
- **Using `str(doc)` for TOML output.** `str()` returns a Python repr.  Use
  `doc.as_toml()` to get TOML text.
- **Forgetting `bump_at` / `bump_self` / `bump_child`.** Every mutation must
  record itself in the trie. Read-only methods do not bump. If a mutation
  changes the *structure* of the target (insert/remove in an array, clear),
  use `bump_self`. If it replaces a *child by key*, use `bump_child`.
- **Re-borrowing inside equality comparisons.** When a method receives a
  `value: &Bound<'_, PyAny>` that might be an `ItemProxy` from the same
  document, calling `__eq__` on it while holding a `borrow_mut()` will panic
  (double borrow). Use `resolve_proxy()` (in `item_proxy.rs`) to extract the
  plain Python value *before* borrowing the document mutably. See
  `ListProxy::remove` for the pattern.
- **Forgetting `check_fresh`.** Every proxy method must call
  `self.check_fresh(&doc)?` before navigating. Without it, a stale proxy
  silently accesses the wrong data or panics.
- **`from __future__ import annotations`** is required in every Python file
  (enforced by ruff). Forgetting it causes CI failure.

