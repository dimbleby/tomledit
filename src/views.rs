//! Live dictionary views (KeysView, ValuesView, ItemsView) for Document and
//! Item proxies.  Each view holds a `Py<Document>` and a key path, just like
//! `ItemProxy`, so it re-navigates on every access and always reflects the
//! current state of the document.

use std::collections::HashSet;

use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PySet, PyTuple};

use crate::document::Document;
use crate::item_ops::{self, Key};
use crate::item_proxy::ItemProxy;

use toml_edit::DocumentMut as DocumentRs;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_key_set(doc: &DocumentRs, path: &[Key]) -> PyResult<HashSet<String>> {
    Ok(get_keys(doc, path)?.into_iter().collect())
}

/// Collect elements from `other` that are strings into a HashSet.
/// Non-string elements are silently ignored (they can never match a key).
fn other_to_string_set(other: &Bound<'_, PyAny>) -> PyResult<HashSet<String>> {
    let mut set = HashSet::new();
    for item in other.try_iter()? {
        if let Ok(s) = item?.extract::<String>() {
            set.insert(s);
        }
    }
    Ok(set)
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

fn get_keys(doc: &DocumentRs, path: &[Key]) -> PyResult<Vec<String>> {
    if path.is_empty() {
        Ok(doc.iter().map(|(k, _)| k.to_owned()).collect())
    } else {
        let item = item_ops::navigate_path(doc, path)?;
        item_ops::item_keys(item)
    }
}

fn get_len(doc: &DocumentRs, path: &[Key]) -> PyResult<usize> {
    if path.is_empty() {
        Ok(doc.len())
    } else {
        let item = item_ops::navigate_path(doc, path)?;
        item_ops::item_len(item).ok_or_else(|| PyTypeError::new_err("TOML item has no len()"))
    }
}

fn contains_key(doc: &DocumentRs, path: &[Key], key: &str) -> PyResult<bool> {
    if path.is_empty() {
        Ok(doc.contains_key(key))
    } else {
        let item = item_ops::navigate_path(doc, path)?;
        item_ops::item_has_key(item, key)
    }
}

// ---------------------------------------------------------------------------
// KeysView
// ---------------------------------------------------------------------------

/// A live view of the string keys in a TOML table (or the document root).
#[pyclass(name = "KeysView", module = "tomledit")]
pub(crate) struct KeysView {
    document: Py<Document>,
    path: Vec<Key>,
}

impl KeysView {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>) -> Self {
        Self { document, path }
    }
}

#[pymethods]
impl KeysView {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        get_len(&doc.inner, &self.path)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        let keys = get_keys(&doc.inner, &self.path)?;
        let list = keys.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        contains_key(&doc.inner, &self.path, key)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        let keys = get_keys(&doc.inner, &self.path)?;
        let inner: Vec<String> = keys.iter().map(|k| format!("'{k}'")).collect();
        Ok(format!("KeysView([{}])", inner.join(", ")))
    }

    fn __reversed__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        let mut keys = get_keys(&doc.inner, &self.path)?;
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
        let doc = self.document.bind(py).borrow();
        let ours = get_key_set(&doc.inner, &self.path)?;
        let theirs = other_to_string_set(other)?;
        let result = &ours & &theirs;
        PySet::new(py, result.iter())
    }

    fn __or__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let doc = self.document.bind(py).borrow();
        let ours = keys_to_pyset(&doc.inner, &self.path, py)?;
        ours.call_method1("__or__", (other,))
    }

    fn __sub__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PySet>> {
        let doc = self.document.bind(py).borrow();
        let ours = get_key_set(&doc.inner, &self.path)?;
        let theirs = other_to_string_set(other)?;
        let result = &ours - &theirs;
        PySet::new(py, result.iter())
    }

    fn __xor__<'py>(
        &self,
        py: Python<'py>,
        other: &Bound<'py, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let doc = self.document.bind(py).borrow();
        let ours = keys_to_pyset(&doc.inner, &self.path, py)?;
        ours.call_method1("__xor__", (other,))
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        let ours = keys_to_pyset(&doc.inner, &self.path, py)?;
        ours.eq(other)
    }
}

// ---------------------------------------------------------------------------
// ValuesView
// ---------------------------------------------------------------------------

/// A live view of the values in a TOML table (or the document root).
#[pyclass(name = "ValuesView", module = "tomledit")]
pub(crate) struct ValuesView {
    document: Py<Document>,
    path: Vec<Key>,
}

impl ValuesView {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>) -> Self {
        Self { document, path }
    }
}

#[pymethods]
impl ValuesView {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        get_len(&doc.inner, &self.path)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        let keys = get_keys(&doc.inner, &self.path)?;
        let generation = doc.trie.clock;
        let proxies: Vec<Py<PyAny>> = keys
            .into_iter()
            .map(|k| {
                ItemProxy::make_child_typed(&self.document, &self.path, generation, py, Key::Str(k))
            })
            .collect::<PyResult<_>>()?;
        let list = proxies.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    fn __contains__(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        let keys = get_keys(&doc.inner, &self.path)?;
        for key in &keys {
            let item = if self.path.is_empty() {
                doc.inner.as_item().get(key.as_str())
            } else {
                item_ops::navigate_path(&doc.inner, &self.path)?.get(key.as_str())
            };
            if let Some(item) = item
                && crate::equality::item_eq(item, value)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        let len = get_len(&doc.inner, &self.path)?;
        Ok(format!("ValuesView(<{len} values>)"))
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let our_iter = self.__iter__(py)?;
        let mut our_iter = our_iter.into_bound(py);
        let other_iter = match other.try_iter() {
            Ok(it) => it,
            Err(_) => return Ok(false),
        };
        for other_val in other_iter {
            let other_val = other_val?;
            match our_iter.next() {
                Some(our_val) => {
                    if !our_val?.eq(&other_val)? {
                        return Ok(false);
                    }
                }
                None => return Ok(false),
            }
        }
        Ok(our_iter.next().is_none())
    }
}

// ---------------------------------------------------------------------------
// ItemsView
// ---------------------------------------------------------------------------

/// A live view of the (key, value) pairs in a TOML table (or the document root).
#[pyclass(name = "ItemsView", module = "tomledit")]
pub(crate) struct ItemsView {
    document: Py<Document>,
    path: Vec<Key>,
}

impl ItemsView {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>) -> Self {
        Self { document, path }
    }
}

#[pymethods]
impl ItemsView {
    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        get_len(&doc.inner, &self.path)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        let keys = get_keys(&doc.inner, &self.path)?;
        let generation = doc.trie.clock;
        let pairs: Vec<(String, Py<PyAny>)> = keys
            .into_iter()
            .map(|k| {
                let obj = ItemProxy::make_child_typed(
                    &self.document,
                    &self.path,
                    generation,
                    py,
                    Key::Str(k.clone()),
                )?;
                Ok((k, obj))
            })
            .collect::<PyResult<_>>()?;
        let list = pairs.into_pyobject(py)?;
        Ok(list.try_iter()?.unbind())
    }

    fn __contains__(&self, py: Python<'_>, item: &Bound<'_, PyAny>) -> PyResult<bool> {
        // ItemsView.__contains__ expects a (key, value) tuple
        let tuple = item
            .cast::<PyTuple>()
            .map_err(|_| PyTypeError::new_err("ItemsView.__contains__ requires a tuple"))?;
        if tuple.len() != 2 {
            return Ok(false);
        }
        let key: String = tuple.get_item(0)?.extract()?;
        let value = tuple.get_item(1)?;

        let doc = self.document.bind(py).borrow();
        let target = if self.path.is_empty() {
            doc.inner.as_item().get(key.as_str())
        } else {
            item_ops::navigate_path(&doc.inner, &self.path)?.get(key.as_str())
        };
        match target {
            Some(item_rs) => crate::equality::item_eq(item_rs, &value),
            None => Ok(false),
        }
    }

    fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        let len = get_len(&doc.inner, &self.path)?;
        Ok(format!("ItemsView(<{len} items>)"))
    }

    fn __eq__(&self, py: Python<'_>, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let our_iter = self.__iter__(py)?;
        let mut our_iter = our_iter.into_bound(py);
        let other_iter = match other.try_iter() {
            Ok(it) => it,
            Err(_) => return Ok(false),
        };
        for other_val in other_iter {
            let other_val = other_val?;
            match our_iter.next() {
                Some(our_val) => {
                    if !our_val?.eq(&other_val)? {
                        return Ok(false);
                    }
                }
                None => return Ok(false),
            }
        }
        Ok(our_iter.next().is_none())
    }
}
