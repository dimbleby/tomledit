use pyo3::prelude::*;
use toml_edit::DocumentMut as DocumentRs;

use crate::document::Document;
use crate::item::Item;
use crate::item_ops::{self, Key};
use crate::item_proxy::{ItemProxy, resolve_proxy};
use crate::list_ops;

/// A TOML array or array of tables.
///
/// ``isinstance(item, ListItem)`` and
/// ``isinstance(item, MutableSequence)`` both work.
#[pyclass(name = "ListItem", module = "tomledit", extends = ItemProxy)]
pub(crate) struct ListProxy;

impl ListProxy {
    fn wrap_in_doc(py: Python<'_>, new_doc: DocumentRs) -> PyResult<Py<PyAny>> {
        let doc_py = Py::new(py, Document::from_inner(new_doc))?;
        let proxy = ItemProxy::new(doc_py, vec![Key::Str("_".to_owned())], 0);
        ItemProxy::into_typed(py, proxy)
    }
}

/// Shared-borrow a proxy's document and clone the underlying `toml_edit::Item`.
fn clone_self_item(base: &ItemProxy, py: Python<'_>) -> PyResult<toml_edit::Item> {
    let doc = base.document.bind(py).borrow();
    base.check_fresh(&doc)?;
    Ok(base.navigate(&doc.inner)?.clone())
}

/// Clone elements from `source` into `dest`, preserving formatting and comments.
///
/// Both items must be the same array kind (both plain arrays, or both AoT).
/// Returns `true` if the types matched and elements were cloned, `false` if
/// the types are incompatible (e.g. array + AoT) — the caller should fall
/// back to value extraction in that case.
fn clone_elements_into(dest: &mut toml_edit::Item, source: &toml_edit::Item, n: usize) -> bool {
    match (dest, source) {
        (
            toml_edit::Item::Value(toml_edit::Value::Array(dest_arr)),
            toml_edit::Item::Value(toml_edit::Value::Array(src_arr)),
        ) => {
            for _ in 0..n {
                for v in src_arr.iter() {
                    dest_arr.push_formatted(v.clone());
                }
            }
            true
        }
        (toml_edit::Item::ArrayOfTables(dest_aot), toml_edit::Item::ArrayOfTables(src_aot)) => {
            for _ in 0..n {
                for t in src_aot.iter() {
                    dest_aot.push(t.clone());
                }
            }
            true
        }
        _ => false,
    }
}

/// Create an empty array-like item matching the kind (array vs AoT) of `source`.
fn empty_array_like(source: list_ops::ArrayLikeRef<'_>) -> toml_edit::Item {
    match source {
        list_ops::ArrayLikeRef::Array(_) => {
            toml_edit::Item::Value(toml_edit::Value::Array(toml_edit::Array::new()))
        }
        list_ops::ArrayLikeRef::Aot(_) => {
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new())
        }
    }
}

/// Extract Python values from an iterable into a `Vec<Item>`.
pub(crate) fn collect_items(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Item>> {
    obj.try_iter()?
        .map(|r| r.and_then(|v| v.extract::<Item>()))
        .collect()
}

#[pymethods]
impl ListProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        crate::item_proxy::parse_as::<ListProxy>(py, text, "ListItem", "array")
    }

    pub fn __iadd__(slf: &Bound<'_, Self>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        Self::extend(slf, values.py(), values)
    }

    pub fn __add__(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !list_ops::is_list_like(other, py) {
            return Ok(py.NotImplemented());
        }
        let mut new_doc = DocumentRs::new();
        new_doc["_"] = clone_self_item(self_.as_super(), py)?;
        // Fast path for same-kind proxies: clone directly to preserve
        // formatting and comments.  Falls back to value extraction for
        // plain lists and cross-kind proxies (array + AoT).
        let fast = if let Ok(proxy) = other.cast::<ItemProxy>() {
            let other_item = clone_self_item(&proxy.borrow(), py)?;
            clone_elements_into(&mut new_doc["_"], &other_item, 1)
        } else {
            false
        };
        if !fast {
            let items = collect_items(other)?;
            let target = list_ops::as_array_like_mut(&mut new_doc["_"], "__add__()")?;
            list_ops::item_extend(target, items)?;
        }
        Self::wrap_in_doc(py, new_doc)
    }

    pub fn __radd__(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !list_ops::is_list_like(other, py) {
            return Ok(py.NotImplemented());
        }
        let items = collect_items(other)?;
        let self_item = clone_self_item(self_.as_super(), py)?;
        let mut new_doc = DocumentRs::new();
        new_doc["_"] = empty_array_like(list_ops::as_array_like(&self_item, "__radd__()")?);
        let target = list_ops::as_array_like_mut(&mut new_doc["_"], "__radd__()")?;
        list_ops::item_extend(target, items)?;
        clone_elements_into(&mut new_doc["_"], &self_item, 1);
        Self::wrap_in_doc(py, new_doc)
    }

    pub fn __mul__(self_: PyRef<'_, Self>, py: Python<'_>, n: isize) -> PyResult<Py<PyAny>> {
        let mut new_doc = DocumentRs::new();
        new_doc["_"] = clone_self_item(self_.as_super(), py)?;
        if n <= 0 {
            item_ops::item_clear(&mut new_doc["_"])?;
        } else if n > 1 {
            let source = new_doc["_"].clone();
            clone_elements_into(&mut new_doc["_"], &source, n as usize - 1);
        }
        Self::wrap_in_doc(py, new_doc)
    }

    pub fn __rmul__(self_: PyRef<'_, Self>, py: Python<'_>, n: isize) -> PyResult<Py<PyAny>> {
        Self::__mul__(self_, py, n)
    }

    pub fn __imul__(mut self_: PyRefMut<'_, Self>, py: Python<'_>, n: isize) -> PyResult<()> {
        if n > 1 {
            let source = clone_self_item(self_.as_super(), py)?;
            let base = self_.into_super();
            let mut doc = base.document.bind(py).borrow_mut();
            base.check_fresh(&doc)?;
            let item = base.navigate_mut(&mut doc.inner)?;
            clone_elements_into(item, &source, n as usize - 1);
        } else if n <= 0 {
            let mut base = self_.into_super();
            let mut doc = base.document.bind(py).borrow_mut();
            base.check_fresh(&doc)?;
            let item = base.navigate_mut(&mut doc.inner)?;
            item_ops::item_clear(item)?;
            base.bump_self(&mut doc);
        }
        Ok(())
    }

    #[pyo3(signature = (index=None, /))]
    pub fn pop(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        index: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        // Resolve proxy index before the mutable borrow — extract() on a
        // ScalarItem triggers __index__ which re-borrows the document.
        let resolved_index = index.map(|i| resolve_proxy(i)).transpose()?.flatten();
        let index = match (&resolved_index, index) {
            (Some(resolved), _) => Some(resolved.bind(py) as &Bound<'_, PyAny>),
            (None, orig) => orig,
        };
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "pop()")?;

        let (removed, affected_key) = list_ops::list_pop(target, index)?;
        let result = item_ops::item_to_py(&removed.0, py)?;
        base.bump_affected(&mut doc, affected_key);
        Ok(result)
    }

    #[pyo3(signature = (value, /))]
    pub fn append(self_: PyRefMut<'_, Self>, py: Python<'_>, value: Item) -> PyResult<()> {
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "append()")?;
        list_ops::item_append(target, value)?;
        Ok(())
    }

    #[pyo3(signature = (index, value, /))]
    pub fn insert(
        self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        index: i64,
        value: Item,
    ) -> PyResult<()> {
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "insert()")?;
        let affected = list_ops::item_insert(target, index, value)?;
        if let Some(a) = affected {
            base.bump_affected(&mut doc, a);
        }
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn remove(
        mut self_: PyRefMut<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        // Phase 1: find the index (shared borrow — value_eq can
        // shared-borrow a proxy's document without conflict).
        let index = {
            let base = self_.as_super();
            let doc = base.document.bind(py).borrow();
            base.check_fresh(&doc)?;
            let item = base.navigate(&doc.inner)?;
            let target = list_ops::as_array_like(item, "remove()")?;
            list_ops::item_index(target, value, None, None)?
        };
        // Phase 2: remove at that index (mutable borrow).
        let mut base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "remove()")?;
        let (_removed, affected_key) = list_ops::item_remove_at(target, index)?;
        base.bump_affected(&mut doc, affected_key);
        Ok(())
    }

    #[pyo3(signature = (values, /))]
    pub fn extend(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        // Collect items BEFORE borrowing the cell — values may be
        // the same proxy, and collect_items invokes __iter__.
        let items = collect_items(values)?;
        let self_mut = slf.borrow_mut();
        let base = self_mut.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "extend()")?;
        list_ops::item_extend(target, items)?;
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn count(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<usize> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        let target = list_ops::as_array_like(item, "count()")?;
        list_ops::item_count(target, value)
    }

    #[pyo3(signature = (value, start=None, stop=None, /))]
    pub fn index(
        self_: PyRef<'_, Self>,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        start: Option<i64>,
        stop: Option<i64>,
    ) -> PyResult<usize> {
        let base = self_.as_super();
        let doc = base.document.bind(py).borrow();
        base.check_fresh(&doc)?;
        let item = base.navigate(&doc.inner)?;
        let target = list_ops::as_array_like(item, "index()")?;
        list_ops::item_index(target, value, start, stop)
    }

    /// Format the array as multiline.
    ///
    /// Each element is placed on its own line, indented by *indent*
    /// spaces, with a trailing comma after the last element.
    /// Use ``.fmt()`` to collapse back to a single line.
    ///
    /// No-op on empty arrays.  Any comments on the array elements will
    /// be removed.
    #[pyo3(signature = (*, indent=4))]
    pub fn set_multiline(self_: PyRefMut<'_, Self>, py: Python<'_>, indent: usize) -> PyResult<()> {
        let base = self_.into_super();
        let mut doc = base.document.bind(py).borrow_mut();
        base.check_fresh(&doc)?;
        let item = base.navigate_mut(&mut doc.inner)?;
        let target = list_ops::as_array_like_mut(item, "set_multiline()")?;
        list_ops::item_set_multiline(target, indent)
    }
}
