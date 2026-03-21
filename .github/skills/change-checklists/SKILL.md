---
name: change-checklists
description: Step-by-step checklists and Rust code patterns for adding methods, classes, modules, or releasing new versions of tomledit. Use when adding new functionality, creating new Python-visible classes, adding Rust modules, or preparing a release.
---

# Change Checklists

## Adding a new property to a Python class

1. **Rust** — add a `#[getter]` (and `#[setter]` if writable) in the
   appropriate `#[pymethods]` block.  For proxy classes, borrow the doc and
   check freshness as with methods.
2. **Type stub** — add the `@property` to `tomledit.pyi`.
3. **Tests** — test in the appropriate `test_*.py`.
4. **Rebuild** — `uv run --reinstall-package tomledit pytest`.

Pattern for a **read-only property** on a proxy:
```rust
#[getter]
pub fn my_prop(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<...> {
    let base = self_.as_super();
    let doc = base.document.bind(py).borrow();
    base.check_fresh(&doc)?;
    let item = base.navigate(&doc.inner)?;
    // ...
}
```

## Adding a new method to an existing Python class

1. **Rust** — add the `#[pymethod]` in the appropriate `#[pymethods]` block:
   - `Document` methods go in `document.rs`.
   - `Item` base methods go in the `#[pymethods] impl ItemProxy` block in
     `item_proxy.rs`.
   - `DictItem`-only methods go in `#[pymethods] impl DictProxy` in
     `dict_proxy.rs`.
   - `ListItem`-only methods go in `#[pymethods] impl ListProxy` in
     `list_proxy.rs`.
   - `ScalarItem`-only methods go in `#[pymethods] impl ScalarProxy` in
     `scalar_proxy.rs`.
   - Heavy logic should be a helper in the corresponding `*_ops.rs` module
     (`dict_ops.rs`, `list_ops.rs`, or `item_ops.rs` for shared/base
     operations); the pymethod should be a thin wrapper that borrows the doc,
     checks freshness, navigates, calls the helper, and bumps.
2. **Type stub** — add the signature to `tomledit.pyi` under the right class.
3. **Tests** — add tests in the appropriate `test_*.py` file.
4. **Rebuild** — `uv run --reinstall-package tomledit pytest` to verify.

Pattern for a **read-only** proxy method:
```rust
pub fn my_method(self_: PyRef<'_, Self>, py: Python<'_>) -> PyResult<...> {
    let base = self_.as_super();
    let doc = base.document.bind(py).borrow();
    base.check_fresh(&doc)?;
    let item = base.navigate(&doc.inner)?;
    item_ops::my_helper(item)
}
```

Pattern for a **mutating** proxy method:
```rust
pub fn my_method(self_: PyRefMut<'_, Self>, py: Python<'_>, ...) -> PyResult<...> {
    let mut base = self_.into_super();
    let mut doc = base.document.bind(py).borrow_mut();
    base.check_fresh(&doc)?;
    let item = base.navigate_mut(&mut doc.inner)?;
    item_ops::my_helper(item, ...)?;
    // bump_self for structural changes (array insert/remove/clear)
    // bump_child for replacing a child by key
    base.bump_self(&mut doc);
    Ok(())
}
```

## Adding a new Python class

1. Define the struct in the appropriate `.rs` file with `#[pyclass]`.
2. Register it in `lib.rs`: `m.add_class::<MyClass>()?`.
3. If it should be an `abc` subclass, add the registration call in the
   `py.run(...)` block in `lib.rs`.
4. Add to `tomledit.pyi`.
5. Add tests.

## Adding a new Rust module

1. Create `src/my_module.rs`.
2. Add `mod my_module;` to `lib.rs`.
3. If it exports PyO3 classes, also add them to the `#[pymodule]` function
   in `lib.rs`.

## Releasing a new version

1. Ensure CI is green on `main`.
2. Update `version` in `Cargo.toml`.
3. Update `CHANGELOG.md` — move items from `## Unreleased` into a new
   `## X.Y.Z (DD Month YYYY)` section.
4. Commit, tag as `vX.Y.Z`, push.  The `publish.yml` workflow builds wheels
   and publishes to PyPI on tag push.
