use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyIterator;
use toml_edit::DocumentMut as DocumentRs;

use crate::item::Item;
use crate::item_ops::{self, Key};
use crate::item_proxy::{
    ItemProxy, ProxyParts, extract_owned_item, resolve_proxy, with_proxy_item, with_resolved_item,
};
use crate::list_ops;

/// A TOML array or array of tables.
///
/// ``isinstance(item, ListItem)`` and
/// ``isinstance(item, MutableSequence)`` both work.
#[pyclass(frozen, name = "ListItem", module = "tomledit", sequence, extends = ItemProxy)]
pub(crate) struct ListProxy;

/// Read a proxy's document and clone the underlying `toml_edit::Item`.
fn clone_self_item(base: &ItemProxy, py: Python<'_>) -> PyResult<toml_edit::Item> {
    let (_doc, inner) = base.read_checked(py)?;
    Ok(base.navigate(&inner)?.clone())
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
fn collect_items(obj: &Bound<'_, PyAny>) -> PyResult<Vec<Item>> {
    obj.try_iter()?
        .map(|r| r.and_then(|v| v.extract::<Item>()))
        .collect()
}

/// A source of elements ready to be appended to an array-like destination,
/// prepared *before* any write lock is held.
///
/// * `ArrayLike`: `obj` was an array-like proxy; its item was cloned once
///   under its own read lock so that the destination's write lock can then
///   cover the entire mutation without re-entering Python iteration.
/// * `Items`: `obj` was a plain iterable; it was drained to a `Vec<Item>`
///   with no locks held.
enum Source {
    ArrayLike(list_ops::ArrayLikeOwned),
    Items(Vec<Item>),
}

/// Resolve `obj` to a `Source`.  Must be called with no locks on the
/// destination document, since it may invoke Python iteration (slow path)
/// or take the source proxy's read lock (fast path).
///
/// Non-array-like proxies (DictProxy, ScalarProxy) fall through to
/// `Items`, matching Python's `list.extend(dict)` semantics (yielding
/// the dict's keys).
fn prepare_source(obj: &Bound<'_, PyAny>) -> PyResult<Source> {
    if let Some(source) = with_proxy_item(obj, list_ops::ArrayLikeOwned::from_item)?.flatten() {
        Ok(Source::ArrayLike(source))
    } else {
        Ok(Source::Items(collect_items(obj)?))
    }
}

/// Append a source to `dest`, preserving formatting when source and dest
/// have compatible kinds (same-kind array-like), falling back to
/// per-element `item_extend` on kind mismatch or a plain iterable.
fn append_source(dest: &mut toml_edit::Item, source: Source, op: &str) -> PyResult<()> {
    let items = match source {
        Source::ArrayLike(source) => {
            if list_ops::clone_elements_into(dest, source.as_ref(), 1) {
                return Ok(());
            }
            source.into_items()
        }
        Source::Items(items) => items,
    };
    let target = list_ops::as_array_like_mut(dest, op)?;
    list_ops::item_extend(target, items)
}

#[pymethods]
impl ListProxy {
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Py<PyAny>> {
        crate::item_proxy::parse_as::<ListProxy>(py, text, "ListItem", "array")
    }

    // ---- container protocol ----

    pub fn __getitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        use list_ops::SubscriptKey;
        let py = key.py();
        let resolved = list_ops::resolve_subscript_key(py, key)?;
        let base = slf.as_super().get();

        match resolved {
            SubscriptKey::Slice(slice) => {
                let parts: Vec<ProxyParts> = {
                    let (doc, inner) = base.read_checked(py)?;
                    let item = base.navigate(&inner)?;
                    let target = list_ops::as_array_like(item, "slicing")?;
                    let si = slice.indices(target.len() as isize)?;
                    let indices = list_ops::collect_slice_indices(si.start, si.stop, si.step);
                    indices
                        .into_iter()
                        .map(|i| base.snapshot_child(doc, &inner, Key::Int(i)))
                        .collect::<PyResult<_>>()?
                };
                let proxies: PyResult<Vec<Py<PyAny>>> = parts
                    .into_iter()
                    .map(|p| p.build(&base.document, py))
                    .collect();
                Ok(proxies?.into_pyobject(py)?.into_any().unbind())
            }
            SubscriptKey::Int(i) => {
                let parts = {
                    let (doc, inner) = base.read_checked(py)?;
                    let item = base.navigate(&inner)?;
                    let idx = list_ops::require_array_index(item, i)?;
                    base.snapshot_child(doc, &inner, Key::Int(idx))?
                };
                parts.build(&base.document, py)
            }
        }
    }

    pub fn __setitem__(
        slf: &Bound<'_, Self>,
        key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        use list_ops::SubscriptKey;
        let py = key.py();
        let resolved = list_ops::resolve_subscript_key(py, key)?;

        match resolved {
            SubscriptKey::Slice(slice) => {
                // Collect items before write lock — value may be the same proxy.
                let values = collect_items(value)?;
                let base = slf.as_super().get();
                let (doc, mut inner) = base.write_checked(py)?;
                let item = base.navigate_mut(&mut inner)?;
                let target = list_ops::as_array_like_mut(item, "slice assignment")?;
                let si = slice.indices(target.len() as isize)?;
                let old_len = target.len();
                let indices = list_ops::collect_slice_indices(si.start, si.stop, si.step);
                let new_count = values.len();
                list_ops::item_setitem_slice(target, si.start, si.stop, si.step, values)?;
                if new_count == indices.len() {
                    for &i in &indices {
                        base.bump_child(doc, Key::Int(i));
                    }
                } else {
                    let from = indices.iter().min().copied().unwrap_or(si.start as usize);
                    base.bump_range(doc, from, old_len);
                }
                Ok(())
            }
            SubscriptKey::Int(i) => {
                // Extract before write lock — value may be a proxy from the same document.
                let value: Item = value.extract()?;
                let base = slf.as_super().get();
                let (doc, mut inner) = base.write_checked(py)?;
                let item = base.navigate_mut(&mut inner)?;
                let target = list_ops::as_array_like_mut(item, "__setitem__")?;
                let replaced_key = list_ops::item_setitem_int(target, i, value)?;
                base.bump_child(doc, replaced_key);
                Ok(())
            }
        }
    }

    pub fn __delitem__(slf: &Bound<'_, Self>, key: &Bound<'_, PyAny>) -> PyResult<()> {
        use list_ops::SubscriptKey;
        let py = key.py();
        let resolved = list_ops::resolve_subscript_key(py, key)?;
        let base = slf.as_super().get();
        let (doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;

        match resolved {
            SubscriptKey::Slice(slice) => {
                let target = list_ops::as_array_like_mut(item, "slice deletion")?;
                let si = slice.indices(target.len() as isize)?;
                let indices = list_ops::collect_slice_indices(si.start, si.stop, si.step);
                if let Some(&min_idx) = indices.iter().min() {
                    let old_len = target.len();
                    list_ops::item_delitem_slice(target, &indices)?;
                    base.bump_range(doc, min_idx, old_len);
                }
                Ok(())
            }
            SubscriptKey::Int(i) => {
                let deleted = list_ops::item_delitem_int(item, i)?;
                base.bump_affected(doc, deleted);
                Ok(())
            }
        }
    }

    pub fn __len__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<usize> {
        let base = slf.as_super().get();
        let (_doc, inner) = base.read_checked(py)?;
        let item = base.navigate(&inner)?;
        let target = list_ops::as_array_like(item, "__len__")?;
        Ok(target.len())
    }

    pub fn __iter__(slf: &Bound<'_, Self>, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let base = slf.as_super().get();
        let parts: Vec<ProxyParts> = {
            let (doc, inner) = base.read_checked(py)?;
            let item = base.navigate(&inner)?;
            let len = list_ops::as_array_like(item, "__iter__")?.len();
            (0..len)
                .map(|i| base.snapshot_child(doc, &inner, Key::Int(i)))
                .collect::<PyResult<_>>()?
        };
        let proxies: PyResult<Vec<Py<PyAny>>> = parts
            .into_iter()
            .map(|p| p.build(&base.document, py))
            .collect();
        let list = proxies?.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    pub fn __contains__(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = value.py();
        let base = slf.as_super().get();
        let doc = base.doc(py);
        Ok(with_resolved_item(
            value,
            doc,
            |d| base.check_fresh(d),
            |inner, needle| {
                let item = base.navigate(inner)?;
                let target = list_ops::as_array_like(item, "'in'")?;
                Ok(list_ops::contains_structural(target, needle))
            },
        )?
        .unwrap_or(false))
    }

    // ---- list-specific methods ----

    pub fn __iadd__(slf: &Bound<'_, Self>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        Self::extend(slf, values.py(), values)
    }

    pub fn __add__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !list_ops::is_list_like(other, py) {
            return Ok(py.NotImplemented());
        }
        let base = slf.as_super().get();
        let source = prepare_source(other)?;
        let mut new_doc = DocumentRs::new();
        new_doc["_"] = clone_self_item(base, py)?;
        append_source(&mut new_doc["_"], source, "__add__()")?;
        ProxyParts::wrap_fresh(new_doc, py)
    }

    pub fn __radd__(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        other: &Bound<'_, PyAny>,
    ) -> PyResult<Py<PyAny>> {
        if !list_ops::is_list_like(other, py) {
            return Ok(py.NotImplemented());
        }
        let base = slf.as_super().get();
        let other_source = prepare_source(other)?;
        let self_item = clone_self_item(base, py)?;
        let self_ref = list_ops::as_array_like(&self_item, "__radd__()")?;
        let mut new_doc = DocumentRs::new();
        new_doc["_"] = empty_array_like(self_ref);
        append_source(&mut new_doc["_"], other_source, "__radd__()")?;
        list_ops::clone_elements_into(&mut new_doc["_"], self_ref, 1);
        ProxyParts::wrap_fresh(new_doc, py)
    }

    pub fn __mul__(slf: &Bound<'_, Self>, py: Python<'_>, n: isize) -> PyResult<Py<PyAny>> {
        let base = slf.as_super().get();
        let mut new_doc = DocumentRs::new();
        new_doc["_"] = clone_self_item(base, py)?;
        if n <= 0 {
            item_ops::item_clear(&mut new_doc["_"])?;
        } else if n > 1 {
            list_ops::item_repeat(&mut new_doc["_"], n as usize - 1, "__mul__()")?;
        }
        ProxyParts::wrap_fresh(new_doc, py)
    }

    pub fn __rmul__(slf: &Bound<'_, Self>, py: Python<'_>, n: isize) -> PyResult<Py<PyAny>> {
        Self::__mul__(slf, py, n)
    }

    pub fn __imul__(slf: &Bound<'_, Self>, py: Python<'_>, n: isize) -> PyResult<()> {
        let base = slf.as_super().get();
        if n == 1 {
            return Ok(());
        }
        let (doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        if n <= 0 {
            item_ops::item_clear(item)?;
            base.bump_self(doc);
        } else {
            list_ops::item_repeat(item, n as usize - 1, "__imul__()")?;
        }
        Ok(())
    }

    #[pyo3(signature = (index=None, /))]
    pub fn pop(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        index: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        // Resolve the index to i64 before the write lock.
        //
        // extract::<i64>() invokes Python's __index__ protocol, which could read the document and
        // deadlock if the write lock is already held.
        let resolved_i64: Option<i64> = match index {
            Some(i) => {
                let resolved = resolve_proxy(i)?;
                let key = resolved.as_ref().map_or(i, |v| v.bind(py));
                Some(key.extract::<i64>()?)
            }
            None => None,
        };
        let base = slf.as_super().get();
        let removed = {
            let (doc, mut inner) = base.write_checked(py)?;
            let item = base.navigate_mut(&mut inner)?;
            let target = list_ops::as_array_like_mut(item, "pop()")?;
            let (removed, affected_key) = list_ops::list_pop(target, resolved_i64)?;
            base.bump_affected(doc, affected_key);
            removed
        };
        item_ops::item_to_py(&removed.0, py)
    }

    #[pyo3(signature = (value, /))]
    pub fn append(slf: &Bound<'_, Self>, py: Python<'_>, value: Item) -> PyResult<()> {
        let base = slf.as_super().get();
        let (_doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        let target = list_ops::as_array_like_mut(item, "append()")?;
        list_ops::item_append(target, value)?;
        Ok(())
    }

    #[pyo3(signature = (index, value, /))]
    pub fn insert(slf: &Bound<'_, Self>, py: Python<'_>, index: i64, value: Item) -> PyResult<()> {
        let base = slf.as_super().get();
        let (doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        let target = list_ops::as_array_like_mut(item, "insert()")?;
        let affected = list_ops::item_insert(target, index, value)?;
        if let Some(a) = affected {
            base.bump_affected(doc, a);
        }
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn remove(slf: &Bound<'_, Self>, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let base = slf.as_super().get();

        // Resolve the needle to an owned `toml_edit::Item` with no destination
        // lock held, then take the write lock for the actual removal.
        let needle = extract_owned_item(value)?
            .ok_or_else(|| PyValueError::new_err("value not in array"))?;

        let (doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        let affected = list_ops::find_and_remove(item, &needle)?;
        base.bump_affected(doc, affected);
        Ok(())
    }

    #[pyo3(signature = (values, /))]
    pub fn extend(
        slf: &Bound<'_, Self>,
        py: Python<'_>,
        values: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let base = slf.as_super().get();
        let source = prepare_source(values)?;
        let (_doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        append_source(item, source, "extend()")
    }

    #[pyo3(signature = (value, /))]
    pub fn count(slf: &Bound<'_, Self>, value: &Bound<'_, PyAny>) -> PyResult<usize> {
        let base = slf.as_super().get();
        let doc = base.doc(value.py());
        Ok(with_resolved_item(
            value,
            doc,
            |d| base.check_fresh(d),
            |inner, needle| {
                let item = base.navigate(inner)?;
                let target = list_ops::as_array_like(item, "count()")?;
                Ok(list_ops::item_count_structural(target, needle))
            },
        )?
        .unwrap_or(0))
    }

    #[pyo3(signature = (value, start=None, stop=None, /))]
    pub fn index(
        slf: &Bound<'_, Self>,
        value: &Bound<'_, PyAny>,
        start: Option<i64>,
        stop: Option<i64>,
    ) -> PyResult<usize> {
        let base = slf.as_super().get();
        let doc = base.doc(value.py());
        with_resolved_item(
            value,
            doc,
            |d| base.check_fresh(d),
            |inner, needle| {
                let item = base.navigate(inner)?;
                let target = list_ops::as_array_like(item, "index()")?;
                list_ops::item_index_structural_range(target, needle, start, stop)
            },
        )?
        .ok_or_else(|| PyValueError::new_err("value not in array"))
    }

    /// Format the array as multiline.
    ///
    /// Each element is placed on its own line, indented by *indent* spaces, with a trailing comma
    /// after the last element. Use ``.fmt()`` to collapse back to a single line.
    ///
    /// No-op on empty arrays.  Any comments on the array elements will be removed.
    #[pyo3(signature = (*, indent=4))]
    pub fn set_multiline(slf: &Bound<'_, Self>, py: Python<'_>, indent: usize) -> PyResult<()> {
        let base = slf.as_super().get();
        let (_doc, mut inner) = base.write_checked(py)?;
        let item = base.navigate_mut(&mut inner)?;
        let target = list_ops::as_array_like_mut(item, "set_multiline()")?;
        list_ops::item_set_multiline(target, indent)
    }
}
