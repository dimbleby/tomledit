use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyIterator, PySlice};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;

use crate::document::Document;
use crate::item::Item;
use crate::ops;

#[derive(Clone)]
pub(crate) enum Key {
    Str(String),
    Int(usize),
}

/// A proxy into a Document that supports chained `__getitem__` / `__setitem__`.
///
/// Instead of cloning the underlying item (which breaks `doc["d"][0] = 7`),
/// ItemProxy holds a reference to the owning Document and a path of keys.
/// Reads and writes navigate that path at call-time so mutations are visible.
#[pyclass(name = "Item", module = "tomledit")]
pub(crate) struct ItemProxy {
    document: Py<Document>,
    path: Vec<Key>,
}

impl ItemProxy {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>) -> Self {
        Self { document, path }
    }

    fn navigate<'a>(&self, doc: &'a DocumentRs) -> PyResult<&'a ItemRs> {
        let mut current: &ItemRs = doc.as_item();
        for key in &self.path {
            let next = match key {
                Key::Str(s) => current.get(s.as_str()),
                Key::Int(i) => current.get(*i),
            };
            current = next.ok_or_else(|| PyKeyError::new_err("path no longer valid"))?;
        }
        Ok(current)
    }

    fn navigate_mut<'a>(&self, doc: &'a mut DocumentRs) -> PyResult<&'a mut ItemRs> {
        let mut current: &mut ItemRs = doc.as_item_mut();
        for key in &self.path {
            let next = match key {
                Key::Str(s) => current.get_mut(s.as_str()),
                Key::Int(i) => current.get_mut(*i),
            };
            current = next.ok_or_else(|| PyKeyError::new_err("path no longer valid"))?;
        }
        Ok(current)
    }

    fn child_proxy(&self, py: Python<'_>, key: Key) -> ItemProxy {
        let mut path = self.path.clone();
        path.push(key);
        ItemProxy {
            document: self.document.clone_ref(py),
            path,
        }
    }
}

#[pymethods]
impl ItemProxy {
    // ---- core protocol ----

    pub fn __getitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        let py = key.py();

        // Slice support: return a list of child proxies.
        if let Ok(slice) = key.cast::<PySlice>() {
            let doc = self.document.bind(py).borrow();
            let item = self.navigate(&doc.0)?;
            let len = ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = ops::collect_slice_indices(si.start, si.stop, si.step);
            let proxies: Vec<ItemProxy> = indices
                .into_iter()
                .map(|i| self.child_proxy(py, Key::Int(i)))
                .collect();
            return Ok(proxies.into_pyobject(py)?.into_any().unbind());
        }

        let new_key = if let Ok(k) = key.extract::<i64>() {
            // Resolve negative indices
            let doc = self.document.bind(py).borrow();
            let item = self.navigate(&doc.0)?;
            let len = ops::item_len(item).ok_or_else(|| {
                PyTypeError::new_err(format!("'{}' is not subscriptable", item.type_name()))
            })?;
            Key::Int(ops::resolve_index(k, len)?)
        } else if let Ok(k) = key.extract::<String>() {
            Key::Str(k)
        } else {
            return Err(ops::bad_key_type(key));
        };

        {
            let doc = self.document.bind(py).borrow();
            let item = self.navigate(&doc.0)?;
            let exists = match &new_key {
                Key::Str(s) => item.get(s.as_str()).is_some(),
                Key::Int(i) => item.get(*i).is_some(),
            };
            if !exists {
                return Err(PyKeyError::new_err(format!("{key}")));
            }
        }

        Ok(self
            .child_proxy(py, new_key)
            .into_pyobject(py)?
            .into_any()
            .unbind())
    }

    pub fn __setitem__(&self, key: &Bound<'_, PyAny>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = key.py();

        if let Ok(slice) = key.cast::<PySlice>() {
            let mut doc = self.document.bind(py).borrow_mut();
            let item = self.navigate_mut(&mut doc.0)?;
            let len = ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let values: Vec<Item> = value
                .try_iter()?
                .map(|r| r.and_then(|v| v.extract::<Item>()))
                .collect::<PyResult<_>>()?;
            return ops::item_setitem_slice(item, si.start, si.stop, si.step, values);
        }

        let value: Item = value.extract()?;
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_setitem(item, key, value)
    }

    pub fn __delitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = key.py();

        if let Ok(slice) = key.cast::<PySlice>() {
            let mut doc = self.document.bind(py).borrow_mut();
            let item = self.navigate_mut(&mut doc.0)?;
            let len = ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = ops::collect_slice_indices(si.start, si.stop, si.step);
            return ops::item_delitem_slice(item, &indices);
        }

        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_delitem(item, key)
    }

    pub fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        ops::item_len(item)
            .ok_or_else(|| PyTypeError::new_err(format!("'{}' has no len()", item.type_name())))
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;

        match ops::item_iter_info(item)? {
            ops::IterKind::TableKeys(keys) => {
                let list = keys.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
            ops::IterKind::ArrayLen(len) => {
                let proxies: Vec<ItemProxy> = (0..len)
                    .map(|i| self.child_proxy(py, Key::Int(i)))
                    .collect();
                let list = proxies.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
        }
    }

    pub fn __contains__(&self, value: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = value.py();
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        ops::item_contains(item, value)
    }

    pub fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        Ok(ops::item_bool(item))
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        ops::item_str(item, py)
    }

    pub fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        Ok(ops::item_repr(item, "Item"))
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = other.py();
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        ops::item_eq(item, other)
    }

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        ops::item_to_py(item, py)
    }

    // ---- dict-like methods ----

    pub fn keys(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        ops::item_keys(item)
    }

    pub fn values(&self, py: Python<'_>) -> PyResult<Vec<ItemProxy>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        let keys = ops::item_keys(item)?;
        Ok(keys
            .into_iter()
            .map(|k| self.child_proxy(py, Key::Str(k)))
            .collect())
    }

    pub fn items(&self, py: Python<'_>) -> PyResult<Vec<(String, ItemProxy)>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        let keys = ops::item_keys(item)?;
        Ok(keys
            .into_iter()
            .map(|k| {
                let proxy = self.child_proxy(py, Key::Str(k.clone()));
                (k, proxy)
            })
            .collect())
    }

    #[pyo3(signature = (key, default=None))]
    pub fn get(
        &self,
        py: Python<'_>,
        key: &str,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        if ops::item_has_key(item, key)? {
            Ok(self
                .child_proxy(py, Key::Str(key.to_owned()))
                .into_pyobject(py)?
                .into_any()
                .unbind())
        } else {
            Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
        }
    }

    #[pyo3(signature = (key=None))]
    pub fn pop(&self, py: Python<'_>, key: Option<&Bound<'_, PyAny>>) -> PyResult<Py<PyAny>> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        let removed = ops::item_pop(item, key)?;
        ops::item_to_py(&removed.0, py)
    }

    pub fn update(&self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = other.py();
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_update(item, other)
    }

    pub fn setdefault(&self, py: Python<'_>, key: &str, default: Item) -> PyResult<ItemProxy> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;

        if !ops::item_has_key(item, key)? {
            ops::set_with_decor_preservation(item, key, default);
        }

        drop(doc);
        Ok(self.child_proxy(py, Key::Str(key.to_owned())))
    }

    // ---- list-like methods ----

    pub fn append(&self, py: Python<'_>, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_append(item, value)
    }

    pub fn insert(&self, py: Python<'_>, index: usize, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_insert(item, index, value)
    }

    pub fn remove(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_remove(item, value)
    }

    pub fn extend(&self, py: Python<'_>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let items: Vec<Item> = values
            .try_iter()?
            .map(|r| r.and_then(|v| v.extract::<Item>()))
            .collect::<PyResult<_>>()?;

        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_extend(item, items)
    }

    // ---- shared methods ----

    pub fn clear(&self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        ops::item_clear(item)
    }
}
