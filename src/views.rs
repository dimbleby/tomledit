//! Live dictionary views (KeysView, ValuesView, ItemsView) for Document and
//! Item proxies.  Each view holds a `Py<Document>`, a key path, and a
//! creation revision — just like `ItemProxy`.  It re-navigates on every
//! access so it always reflects the current state of the document, but goes
//! stale (raises `RuntimeError`) when the path itself has been invalidated
//! by a mutation.

use std::collections::HashSet;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PyList, PySet, PyTuple};

use crate::dict_ops;
use crate::document::Document;
use crate::equality;
use crate::item_ops::{self, Key};
use crate::item_proxy::{ItemProxy, with_resolved_item};

use toml_edit::DocumentMut as DocumentRs;

/// Adds `read_checked` to a view type with `document`, `path`, `revision`
/// fields.  Locks `inner.read()` first, then verifies freshness.  Closes
/// the TOCTOU window between the check and the read.
macro_rules! impl_view_read_checked {
    ($ty:ty) => {
        impl $ty {
            pub(crate) fn read_checked<'py>(
                &'py self,
                py: Python<'py>,
            ) -> PyResult<(&'py Document, parking_lot::RwLockReadGuard<'py, DocumentRs>)> {
                let doc = self.document.bind(py).get();
                let guard = doc.read_checked(&self.path, self.revision)?;
                Ok((doc, guard))
            }
        }
    };
}

fn get_key_set(doc: &DocumentRs, path: &[Key]) -> PyResult<HashSet<String>> {
    Ok(get_keys(doc, path)?.into_iter().collect())
}

/// Collect elements from `other` that are strings (or string-valued proxies)
/// into a HashSet.  Non-string elements are silently ignored.
fn other_to_string_set(other: &Bound<'_, PyAny>) -> PyResult<HashSet<String>> {
    let mut set = HashSet::new();
    for item in other.try_iter()? {
        if let Some(s) = item_ops::extract_key_str(&item?)? {
            set.insert(s);
        }
    }
    Ok(set)
}

/// Convert any Python iterable to a `set`.  If `other` is already a set,
/// this is a no-op; otherwise it calls `set(other)`.
fn iterable_to_pyset<'py>(
    py: Python<'py>,
    other: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PySet>> {
    if let Ok(s) = other.cast::<PySet>() {
        return Ok(s.clone());
    }
    let set_type = py.get_type::<PySet>();
    set_type.call1((other,))?.cast_into().map_err(Into::into)
}

/// Build a Python set from our string keys.
fn keys_to_pyset<'py>(
    doc: &DocumentRs,
    path: &[Key],
    py: Python<'py>,
) -> PyResult<Bound<'py, PySet>> {
    let keys = get_keys(doc, path)?;
    PySet::new(py, keys.iter())
}

/// Build a Python set of `(key, value)` tuples from the table at `path`.
///
/// Values are converted via `item_to_py`; non-hashable values (tables,
/// arrays) cause `PySet::add` to raise `TypeError`, matching the behavior
/// of `dict.items()` on the same input.
/// Build a `(key, item_to_py(item))` Python tuple.
fn entry_to_pytuple<'py>(
    py: Python<'py>,
    key: &str,
    item: &toml_edit::Item,
) -> PyResult<Bound<'py, PyTuple>> {
    let py_val = item_ops::item_to_py(item, py)?;
    PyTuple::new(
        py,
        [key.into_pyobject(py)?.into_any(), py_val.into_bound(py)],
    )
}

fn items_to_pyset<'py>(
    doc: &DocumentRs,
    path: &[Key],
    py: Python<'py>,
) -> PyResult<Bound<'py, PySet>> {
    let parent = item_ops::navigate_path(doc, path)?;
    let set = PySet::empty(py)?;
    dict_ops::for_each_entry(parent, |key, item| {
        set.add(entry_to_pytuple(py, key, item)?)?;
        Ok(())
    })?;
    Ok(set)
}

fn get_keys(doc: &DocumentRs, path: &[Key]) -> PyResult<Vec<String>> {
    let item = item_ops::navigate_path(doc, path)?;
    dict_ops::item_keys(item)
}

/// Build a Python list of key strings directly from the TOML iterator,
/// without an intermediate Rust Vec.
fn keys_to_pylist<'py>(
    doc: &DocumentRs,
    path: &[Key],
    py: Python<'py>,
) -> PyResult<Bound<'py, PyList>> {
    let item = item_ops::navigate_path(doc, path)?;
    let list = PyList::empty(py);
    dict_ops::for_each_key(item, |k| list.append(k))?;
    Ok(list)
}

fn get_len(doc: &DocumentRs, path: &[Key]) -> PyResult<usize> {
    let item = item_ops::navigate_path(doc, path)?;
    let tbl = item
        .as_table_like()
        .ok_or_else(|| PyTypeError::new_err("TOML item has no len()"))?;
    Ok(tbl.len())
}

fn contains_key(doc: &DocumentRs, path: &[Key], key: &str) -> PyResult<bool> {
    let item = item_ops::navigate_path(doc, path)?;
    dict_ops::item_has_key(item, key)
}

/// Invoke `f(key, child_proxy)` for every entry in the table at `path`.
///
/// Shared by `ValuesView` and `ItemsView` for both `__iter__` and
/// `__reversed__`.  Keys are collected under the read lock, then proxies
/// are built individually (each `make_child_typed` takes its own short-lived
/// lock) to avoid a recursive `RwLock::read()`.
fn with_child_proxies(
    document: &Py<Document>,
    path: &[Key],
    view_revision: u64,
    py: Python<'_>,
    mut f: impl FnMut(&str, Py<PyAny>) -> PyResult<()>,
) -> PyResult<()> {
    let doc = document.bind(py).get();
    let (keys, revision) = {
        let inner = doc.inner.read();
        doc.check_fresh(path, view_revision)?;
        let revision = doc.revision();
        let item = item_ops::navigate_path(&inner, path)?;
        (dict_ops::item_keys(item)?, revision)
    };
    for k in &keys {
        let proxy = ItemProxy::make_child_typed(document, path, revision, py, Key::Str(k.clone()))?;
        f(k, proxy)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// KeysView
// ---------------------------------------------------------------------------

/// A live view of the string keys in a TOML table (or the document root).
#[pyclass(frozen, name = "KeysView", module = "tomledit")]
pub(crate) struct KeysView {
    document: Py<Document>,
    path: Vec<Key>,
    revision: u64,
}

impl KeysView {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>, revision: u64) -> Self {
        Self {
            document,
            path,
            revision,
        }
    }
}

impl_view_read_checked!(KeysView);

#[pymethods]
impl KeysView {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let (_doc, inner) = self.read_checked(py)?;
        get_len(&inner, &self.path)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let (_doc, inner) = self.read_checked(py)?;
        let list = keys_to_pylist(&inner, &self.path, py)?;
        Ok(list.try_iter()?.unbind())
    }

    fn __contains__(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let Some(key) = crate::item_ops::extract_key_str(key)? else {
            return Ok(false);
        };
        let (_doc, inner) = self.read_checked(py)?;
        contains_key(&inner, &self.path, &key)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let (_doc, inner) = self.read_checked(py)?;
        let keys = get_keys(&inner, &self.path)?;
        let inner_str: Vec<String> = keys.iter().map(|k| format!("'{k}'")).collect();
        Ok(format!("KeysView([{}])", inner_str.join(", ")))
    }

    fn __reversed__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let (_doc, inner) = self.read_checked(py)?;
        let mut keys = get_keys(&inner, &self.path)?;
        keys.reverse();
        let list = keys.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    // Set operations.
    // __and__ and __sub__ always produce subsets of our keys (strings),
    // so we stay in Rust.  __or__ and __xor__ can include arbitrary
    // elements from `other`, so we delegate to Python set operations.

    fn __and__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PySet>> {
        let theirs = other_to_string_set(other)?;
        let (_doc, inner) = self.read_checked(py)?;
        let ours = get_key_set(&inner, &self.path)?;
        let result = &ours & &theirs;
        PySet::new(py, result.iter())
    }

    fn __or__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            keys_to_pyset(&inner, &self.path, py)?
        };
        ours.call_method1("__or__", (theirs,))
    }

    fn __sub__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PySet>> {
        let theirs = other_to_string_set(other)?;
        let (_doc, inner) = self.read_checked(py)?;
        let ours = get_key_set(&inner, &self.path)?;
        let result = &ours - &theirs;
        PySet::new(py, result.iter())
    }

    fn __xor__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            keys_to_pyset(&inner, &self.path, py)?
        };
        ours.call_method1("__xor__", (theirs,))
    }

    // Reflected set operators for `set() OP keys`.  `|`, `&` and `^` are
    // commutative, so they delegate to the forward methods.  `-` is not:
    // `other - keys` subtracts *our* keys from `other`, which we do by
    // building a Python set of our keys and calling its `__sub__`.

    fn __ror__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.__or__(py, other)
    }

    fn __rand__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PySet>> {
        self.__and__(py, other)
    }

    fn __rxor__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.__xor__(py, other)
    }

    fn __rsub__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            keys_to_pyset(&inner, &self.path, py)?
        };
        theirs.call_method1("__sub__", (ours,))
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            keys_to_pyset(&inner, &self.path, py)?
        };
        ours.eq(other)
    }
}

// ---------------------------------------------------------------------------
// ValuesView
// ---------------------------------------------------------------------------

/// A live view of the values in a TOML table (or the document root).
#[pyclass(frozen, name = "ValuesView", module = "tomledit")]
pub(crate) struct ValuesView {
    document: Py<Document>,
    path: Vec<Key>,
    revision: u64,
}

impl ValuesView {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>, revision: u64) -> Self {
        Self {
            document,
            path,
            revision,
        }
    }
}

impl_view_read_checked!(ValuesView);

#[pymethods]
impl ValuesView {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let (_doc, inner) = self.read_checked(py)?;
        get_len(&inner, &self.path)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let list = PyList::empty(py);
        with_child_proxies(&self.document, &self.path, self.revision, py, |_, proxy| {
            list.append(proxy)
        })?;
        Ok(list.try_iter()?.unbind())
    }

    fn __contains__(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let doc = self.document.bind(py).get();
        Ok(with_resolved_item(
            value,
            doc,
            |d| d.check_fresh(&self.path, self.revision),
            |inner, needle| {
                let parent = item_ops::navigate_path(inner, &self.path)?;
                let tbl = dict_ops::as_dict_like(parent, "__contains__")?;
                Ok(tbl
                    .iter()
                    .any(|(_, item)| equality::items_structural_eq(item, needle)))
            },
        )?
        .unwrap_or(false))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let (_doc, inner) = self.read_checked(py)?;
        let len = get_len(&inner, &self.path)?;
        Ok(format!("ValuesView(<{len} values>)"))
    }

    fn __reversed__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let mut proxies = Vec::new();
        with_child_proxies(&self.document, &self.path, self.revision, py, |_, proxy| {
            proxies.push(proxy);
            Ok(())
        })?;
        proxies.reverse();
        let list = PyList::empty(py);
        for proxy in proxies {
            list.append(proxy)?;
        }
        Ok(list.try_iter()?.unbind())
    }

    // No __eq__: Python's dict_values has no equality support (returns
    // NotImplemented), falling back to identity comparison.  We match that
    // by simply not defining __eq__.
}

// ---------------------------------------------------------------------------
// ItemsView
// ---------------------------------------------------------------------------

/// A live view of the (key, value) pairs in a TOML table (or the document root).
#[pyclass(frozen, name = "ItemsView", module = "tomledit")]
pub(crate) struct ItemsView {
    document: Py<Document>,
    path: Vec<Key>,
    revision: u64,
}

impl ItemsView {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>, revision: u64) -> Self {
        Self {
            document,
            path,
            revision,
        }
    }
}

impl_view_read_checked!(ItemsView);

#[pymethods]
impl ItemsView {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let (_doc, inner) = self.read_checked(py)?;
        get_len(&inner, &self.path)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let list = PyList::empty(py);
        with_child_proxies(&self.document, &self.path, self.revision, py, |k, proxy| {
            list.append((k, proxy.into_bound(py)).into_pyobject(py)?)
        })?;
        Ok(list.try_iter()?.unbind())
    }

    fn __contains__(&self, py: Python<'_>, item: &Bound<'_, PyAny>) -> PyResult<bool> {
        // ItemsView.__contains__ expects a (key, value) tuple.
        // Return False (not TypeError) for non-tuples or wrong shapes,
        // matching Python's dict_items behavior.
        let Ok(tuple) = item.cast::<PyTuple>() else {
            return Ok(false);
        };
        if tuple.len() != 2 {
            return Ok(false);
        }
        let key_obj = tuple.get_item(0)?;
        let Some(key) = item_ops::extract_key_str(&key_obj)? else {
            return Ok(false);
        };
        let value = tuple.get_item(1)?;

        let doc = self.document.bind(py).get();
        Ok(with_resolved_item(
            &value,
            doc,
            |d| d.check_fresh(&self.path, self.revision),
            |inner, needle| {
                let target = item_ops::navigate_path(inner, &self.path)?.get(&key);
                Ok(target.is_some_and(|item_rs| equality::items_structural_eq(item_rs, needle)))
            },
        )?
        .unwrap_or(false))
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let (_doc, inner) = self.read_checked(py)?;
        let len = get_len(&inner, &self.path)?;
        Ok(format!("ItemsView(<{len} items>)"))
    }

    fn __reversed__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let mut pairs = Vec::new();
        with_child_proxies(&self.document, &self.path, self.revision, py, |k, proxy| {
            pairs.push((k.to_owned(), proxy));
            Ok(())
        })?;
        pairs.reverse();
        let list = PyList::empty(py);
        for (k, proxy) in pairs {
            list.append((k, proxy.into_bound(py)).into_pyobject(py)?)?;
        }
        Ok(list.try_iter()?.unbind())
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        // ItemsView uses set semantics: only equal to Set-like objects
        // (sets, frozensets, other views registered as Set ABCs).
        let set_abc = py.import("collections.abc")?.getattr("Set")?;
        if !other.is_instance(&set_abc)? {
            return Ok(false);
        }

        let other_len = other.len()?;

        // Build plain Python (key, value) tuples under the read lock, then
        // release it before calling into Python's __contains__ (which could
        // re-enter our code if `other` is a view from the same document).
        let pairs = {
            let (_doc, inner) = self.read_checked(py)?;
            let our_len = get_len(&inner, &self.path)?;
            if our_len != other_len {
                return Ok(false);
            }
            let parent = item_ops::navigate_path(&inner, &self.path)?;
            let mut pairs = Vec::with_capacity(our_len);
            dict_ops::for_each_entry(parent, |key, item| {
                pairs.push(entry_to_pytuple(py, key, item)?);
                Ok(())
            })?;
            pairs
        };

        for pair in &pairs {
            if !other.contains(pair)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    // Set operations.  Items are (key, value) tuples; we build a Python set
    // of them and delegate.  Unhashable values raise `TypeError` during set
    // construction — matching Python's `dict_items` semantics.

    fn __or__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            items_to_pyset(&inner, &self.path, py)?
        };
        ours.call_method1("__or__", (theirs,))
    }

    fn __and__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            items_to_pyset(&inner, &self.path, py)?
        };
        ours.call_method1("__and__", (theirs,))
    }

    fn __sub__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            items_to_pyset(&inner, &self.path, py)?
        };
        ours.call_method1("__sub__", (theirs,))
    }

    fn __xor__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            items_to_pyset(&inner, &self.path, py)?
        };
        ours.call_method1("__xor__", (theirs,))
    }

    // Reflected forms: `|`, `&`, `^` are commutative; `-` is not.

    fn __ror__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.__or__(py, other)
    }

    fn __rand__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.__and__(py, other)
    }

    fn __rxor__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.__xor__(py, other)
    }

    fn __rsub__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let theirs = iterable_to_pyset(py, other)?;
        let ours = {
            let (_doc, inner) = self.read_checked(py)?;
            items_to_pyset(&inner, &self.path, py)?
        };
        theirs.call_method1("__sub__", (ours,))
    }
}
