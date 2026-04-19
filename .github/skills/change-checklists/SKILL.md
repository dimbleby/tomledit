---
name: change-checklists
description: Step-by-step checklists for adding methods, properties, classes, or modules to tomledit, and for cutting a release. Use when adding new functionality or preparing a release.
---

# Change Checklists

These are procedural checklists.
For the Rust patterns themselves (lock acquisition via `read_checked` /
`write_checked`, freshness, the `bump_self` / `bump_child` distinction,
lock-conflict avoidance), see `.github/copilot-instructions.md` and existing
methods in `dict_proxy.rs` / `list_proxy.rs`.

## Adding a property to a Python class

1.  **Rust** — add a `#[getter]` (and `#[setter]` if writable) in the appropriate
    `#[pymethods]` block.
2.  **Type stub** — add the `@property` to `tomledit.pyi`.
3.  **Tests** — add tests in the appropriate `tests/test_*.py`.
4.  **Rebuild and run** — `make build && uv run pytest` (or `make test`).

## Adding a method to an existing Python class

1.  **Rust** — add the `#[pymethod]` in the right block:
    - `Document` methods → `document.rs`.
    - `Item` base methods → `#[pymethods] impl ItemProxy` in `item_proxy.rs`.
    - `DictItem` / `ListItem` / `ScalarItem` methods → the matching `*_proxy.rs`.
    - Heavy logic belongs in the corresponding `*_ops.rs` helper; the pymethod
      should be a thin wrapper.
2.  **Type stub** — add the signature to `tomledit.pyi` under the right class.
3.  **Tests** — add tests in the appropriate `tests/test_*.py`.
4.  **Rebuild and run** — `make build && uv run pytest` (or `make test`).

## Adding a new Python class

1.  Define the struct in the appropriate `.rs` file with `#[pyclass(frozen)]`.
2.  Register it in `lib.rs`: `m.add_class::<MyClass>()?`.
3.  If it should be an `abc` subclass, add the registration call in the
    `py.run(...)` block in `lib.rs`.
4.  Add to `tomledit.pyi`.
5.  Add tests.

## Adding a new Rust module

1.  Create `src/my_module.rs`.
2.  Add `mod my_module;` to `lib.rs`.
3.  If it exports PyO3 classes, also add them to the `#[pymodule]` function in
    `lib.rs`.

## Releasing a new version

1.  Ensure CI is green on `main`.
2.  Update `version` in `Cargo.toml`.
3.  Update `CHANGELOG.md` — move items from `## Unreleased` into a new `## X.Y.Z (DD
Month YYYY)` section.
4.  Commit, tag as `vX.Y.Z`, push.
    The `publish.yml` workflow builds wheels and publishes to PyPI on tag push.
