use pyo3::exceptions::{PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyIterator, PySlice, PyTuple};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::document::Document;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{self, Key};

fn navigate_path<'a>(doc: &'a DocumentRs, path: &[Key]) -> PyResult<&'a ItemRs> {
    let mut current: &ItemRs = doc.as_item();
    for key in path {
        let next = match key {
            Key::Str(s) => current.get(s.as_str()),
            Key::Int(i) => current.get(*i),
        };
        current = next.ok_or_else(|| PyKeyError::new_err("path no longer valid"))?;
    }
    Ok(current)
}

fn navigate_path_mut<'a>(doc: &'a mut DocumentRs, path: &[Key]) -> PyResult<&'a mut ItemRs> {
    let mut current: &mut ItemRs = doc.as_item_mut();
    for key in path {
        let next = match key {
            Key::Str(s) => current.get_mut(s.as_str()),
            Key::Int(i) => current.get_mut(*i),
        };
        current = next.ok_or_else(|| PyKeyError::new_err("path no longer valid"))?;
    }
    Ok(current)
}

/// A reference to a value inside a Document (table, array, or scalar).
///
/// Items are obtained by indexing a Document or another Item — they are
/// live views, so ``doc["server"]["port"] = 8080`` modifies the document
/// in place.  Use ``.value`` to get a plain Python object, or dict/list
/// methods like ``keys()``, ``append()``, etc. to edit in place.
///
/// An Item becomes stale if the document is modified through a different
/// reference; using a stale Item raises ``RuntimeError``.
#[pyclass(name = "Item", module = "tomledit")]
pub(crate) struct ItemProxy {
    document: Py<Document>,
    path: Vec<Key>,
    generation: u64,
}

impl ItemProxy {
    pub(crate) fn new(document: Py<Document>, path: Vec<Key>, generation: u64) -> Self {
        Self {
            document,
            path,
            generation,
        }
    }

    /// Check that the document hasn't been mutated since this proxy was created.
    fn check_generation(&self, doc: &Document) -> PyResult<()> {
        if self.generation != doc.generation {
            Err(PyRuntimeError::new_err(
                "this Item is stale: the document has been modified since it was created",
            ))
        } else {
            Ok(())
        }
    }

    /// Bump the document's generation and update our own snapshot,
    /// so this proxy remains valid after its own mutation.
    fn bump_generation(&mut self, doc: &mut Document) {
        doc.bump();
        self.generation = doc.generation;
    }

    /// Clone the toml_edit item at this proxy's path.
    ///
    /// For array elements, the inline comment (stored externally in the array's
    /// slot system) is embedded into the cloned value's decor suffix so that it
    /// travels with the value when inserted into another array.
    pub(crate) fn clone_item(&self, py: Python<'_>) -> PyResult<ItemRs> {
        let doc = self.document.borrow(py);
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        let mut cloned = item.clone();
        if let Some(Key::Int(idx)) = self.path.last() {
            let parent = self.navigate_parent(&doc.inner)?;
            if let Some(comment) = comments::get_array_item_comment(parent, *idx)
                && let Some(v) = cloned.as_value_mut()
            {
                v.decor_mut().set_suffix(format!(" {comment}"));
            }
        }
        Ok(cloned)
    }

    fn navigate<'a>(&self, doc: &'a DocumentRs) -> PyResult<&'a ItemRs> {
        navigate_path(doc, &self.path)
    }

    fn navigate_mut<'a>(&self, doc: &'a mut DocumentRs) -> PyResult<&'a mut ItemRs> {
        navigate_path_mut(doc, &self.path)
    }

    fn child_proxy(&self, py: Python<'_>, key: Key) -> ItemProxy {
        let mut path = self.path.clone();
        path.push(key);
        ItemProxy {
            document: self.document.clone_ref(py),
            path,
            generation: self.generation,
        }
    }

    /// Navigate to the parent item (all path segments except the last).
    fn navigate_parent<'a>(&self, doc: &'a DocumentRs) -> PyResult<&'a ItemRs> {
        navigate_path(doc, &self.path[..self.path.len() - 1])
    }

    fn navigate_parent_mut<'a>(&self, doc: &'a mut DocumentRs) -> PyResult<&'a mut ItemRs> {
        navigate_path_mut(doc, &self.path[..self.path.len() - 1])
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
            self.check_generation(&doc)?;
            let item = self.navigate(&doc.inner)?;
            let len = item_ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = item_ops::collect_slice_indices(si.start, si.stop, si.step);
            let proxies: Vec<ItemProxy> = indices
                .into_iter()
                .map(|i| self.child_proxy(py, Key::Int(i)))
                .collect();
            return Ok(proxies.into_pyobject(py)?.into_any().unbind());
        }

        let new_key = {
            let doc = self.document.bind(py).borrow();
            self.check_generation(&doc)?;
            let item = self.navigate(&doc.inner)?;
            item_ops::item_getitem(item, key)?
        };

        Ok(self
            .child_proxy(py, new_key)
            .into_pyobject(py)?
            .into_any()
            .unbind())
    }

    pub fn __setitem__(
        &mut self,
        key: &Bound<'_, PyAny>,
        value: &Bound<'_, PyAny>,
    ) -> PyResult<()> {
        let py = key.py();

        if let Ok(slice) = key.cast::<PySlice>() {
            let values: Vec<Item> = value
                .try_iter()?
                .map(|r| r.and_then(|v| v.extract::<Item>()))
                .collect::<PyResult<_>>()?;

            let mut doc = self.document.bind(py).borrow_mut();
            self.check_generation(&doc)?;
            let item = self.navigate_mut(&mut doc.inner)?;
            let len = item_ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            item_ops::item_setitem_slice(item, si.start, si.stop, si.step, values)?;
            self.bump_generation(&mut doc);
            return Ok(());
        }

        let value: Item = value.extract()?;
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        let replaced = item_ops::item_setitem(item, key, value)?;
        if replaced {
            self.bump_generation(&mut doc);
        }
        Ok(())
    }

    pub fn __delitem__(&mut self, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = key.py();

        if let Ok(slice) = key.cast::<PySlice>() {
            let mut doc = self.document.bind(py).borrow_mut();
            self.check_generation(&doc)?;
            let item = self.navigate_mut(&mut doc.inner)?;
            let len = item_ops::require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = item_ops::collect_slice_indices(si.start, si.stop, si.step);
            item_ops::item_delitem_slice(item, &indices)?;
            self.bump_generation(&mut doc);
            return Ok(());
        }

        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_delitem(item, key)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    pub fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_len(item).ok_or_else(|| {
            PyTypeError::new_err(format!(
                "TOML {} item has no len() (use .value to get the Python object)",
                item.type_name()
            ))
        })
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;

        match item_ops::item_iter_info(item)? {
            item_ops::IterKind::TableKeys(keys) => {
                let list = keys.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
            item_ops::IterKind::ArrayLen(len) => {
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
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_contains(item, value)
    }

    pub fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_ops::item_bool(item))
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_str(item, py)
    }

    pub fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_ops::item_repr(item))
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = other.py();

        // Proxy-vs-proxy: compare underlying items directly in Rust.
        if let Ok(other_proxy) = other.cast::<Self>() {
            let other_proxy = other_proxy.borrow();
            let doc = self.document.bind(py).borrow();
            self.check_generation(&doc)?;
            let self_item = self.navigate(&doc.inner)?;
            let other_doc = other_proxy.document.bind(py).borrow();
            other_proxy.check_generation(&other_doc)?;
            let other_item = other_proxy.navigate(&other_doc.inner)?;
            return Ok(equality::items_structural_eq(self_item, other_item));
        }

        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        equality::item_eq(item, other)
    }

    pub fn __iadd__(&mut self, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = values.py();
        let items: Vec<Item> = values
            .try_iter()?
            .map(|r| r.and_then(|v| v.extract::<Item>()))
            .collect::<PyResult<_>>()?;

        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_extend(item, items, "+=")
    }

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_to_py(item, py)
    }

    // ---- comment access ----

    /// The comment lines before this entry, or None.
    #[getter]
    pub fn comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let Some(last_key) = self.path.last() else {
            return Ok(None);
        };
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        match last_key {
            Key::Str(key_str) => {
                let parent = self.navigate_parent(&doc.inner)?;
                Ok(comments::get_key_prefix_comment(parent, key_str))
            }
            Key::Int(_) => {
                let item = self.navigate(&doc.inner)?;
                Ok(comments::get_value_prefix_comment(item))
            }
        }
    }

    /// Set or clear the block comment above this entry.
    ///
    /// Each non-empty line must start with ``#``.  Pass ``None`` to remove
    /// the comment.  Empty lines in the string produce blank lines above
    /// the entry.
    #[setter]
    pub fn set_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        let Some(last_key) = self.path.last() else {
            return Err(PyTypeError::new_err("cannot set comment on root"));
        };
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        match last_key {
            Key::Str(key_str) => {
                let parent = self.navigate_parent_mut(&mut doc.inner)?;
                comments::set_key_prefix_comment(parent, key_str, value)?;
            }
            Key::Int(_) => {
                let item = self.navigate_mut(&mut doc.inner)?;
                comments::set_value_prefix_comment(item, value)?;
            }
        }
        Ok(())
    }

    /// The inline comment after this value (e.g. `key = 1 # this part`), or None.
    #[getter]
    pub fn inline_comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        if let Some(Key::Int(idx)) = self.path.last() {
            let parent = self.navigate_parent(&doc.inner)?;
            return Ok(comments::get_array_item_comment(parent, *idx));
        }
        let item = self.navigate(&doc.inner)?;
        Ok(comments::get_suffix_comment(item))
    }

    /// Set or clear the inline comment on this entry.
    ///
    /// The value must start with ``#`` (e.g. ``"# my note"``).
    /// Pass ``None`` to remove the comment.
    #[setter]
    pub fn set_inline_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        if let Some(Key::Int(idx)) = self.path.last() {
            let parent = self.navigate_parent_mut(&mut doc.inner)?;
            let array = parent
                .as_value_mut()
                .and_then(|v| v.as_array_mut())
                .ok_or_else(|| PyTypeError::new_err("parent is not an array"))?;
            let raw = match value {
                Some(text) => comments::validate_inline_comment(text)?,
                None => String::new(),
            };
            comments::set_array_item_comment(array, *idx, &raw);
            return Ok(());
        }
        // Inline comments inside single-line inline tables would produce
        // invalid TOML (the # eats the rest of the line including `}`).
        if value.is_some() && self.path.len() >= 2 {
            let parent = self.navigate_parent(&doc.inner)?;
            if parent.is_inline_table() {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "cannot set inline comment on inline table value \
                     (comment would consume the closing `}`)",
                ));
            }
        }
        let item = self.navigate_mut(&mut doc.inner)?;
        comments::set_suffix_comment(item, value)?;
        Ok(())
    }

    // ---- dict-like methods ----

    pub fn keys(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_keys(item)
    }

    pub fn values(&self, py: Python<'_>) -> PyResult<Vec<ItemProxy>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        let keys = item_ops::item_keys(item)?;
        Ok(keys
            .into_iter()
            .map(|k| self.child_proxy(py, Key::Str(k)))
            .collect())
    }

    pub fn items(&self, py: Python<'_>) -> PyResult<Vec<(String, ItemProxy)>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        let keys = item_ops::item_keys(item)?;
        Ok(keys
            .into_iter()
            .map(|k| {
                let proxy = self.child_proxy(py, Key::Str(k.clone()));
                (k, proxy)
            })
            .collect())
    }

    #[pyo3(signature = (key, default=None, /))]
    pub fn get(
        &self,
        py: Python<'_>,
        key: &str,
        default: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        if item_ops::item_has_key(item, key)? {
            Ok(self
                .child_proxy(py, Key::Str(key.to_owned()))
                .into_pyobject(py)?
                .into_any()
                .unbind())
        } else {
            Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
        }
    }

    #[pyo3(signature = (*args))]
    pub fn pop(&mut self, py: Python<'_>, args: &Bound<'_, PyTuple>) -> PyResult<Py<PyAny>> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;

        let max_args: usize = if matches!(
            item,
            ItemRs::Value(ValueRs::Array(_)) | ItemRs::ArrayOfTables(_)
        ) {
            1
        } else {
            2
        };
        if args.len() > max_args {
            return Err(PyTypeError::new_err(format!(
                "pop expected at most {} argument{}, got {}",
                max_args,
                if max_args == 1 { "" } else { "s" },
                args.len()
            )));
        }

        let key_obj = if args.is_empty() {
            None
        } else {
            Some(args.get_item(0)?)
        };
        let default = if args.len() == 2 {
            Some(args.get_item(1)?.unbind())
        } else {
            None
        };

        match item_ops::item_pop(item, key_obj.as_ref()) {
            Ok(removed) => {
                let result = item_ops::item_to_py(&removed.0, py)?;
                self.bump_generation(&mut doc);
                Ok(result)
            }
            Err(e) if default.is_some() && e.is_instance_of::<PyKeyError>(py) => {
                Ok(default.unwrap())
            }
            Err(e) => Err(e),
        }
    }

    #[pyo3(signature = (other=None, /, **kwargs))]
    pub fn update(
        &mut self,
        py: Python<'_>,
        other: Option<&Bound<'_, PyAny>>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<()> {
        let mut pairs = match other {
            Some(obj) => item_ops::extract_update_pairs(obj)?,
            None => Vec::new(),
        };
        if let Some(kw) = kwargs {
            for (k, v) in kw.iter() {
                let key: String = k.extract()?;
                let val: Item = v.extract()?;
                pairs.push((key, val));
            }
        }
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        let replaced = item_ops::apply_update_pairs(item, pairs)?;
        if replaced {
            self.bump_generation(&mut doc);
        }
        Ok(())
    }

    #[pyo3(signature = (key, default, /))]
    pub fn setdefault(&mut self, py: Python<'_>, key: &str, default: Item) -> PyResult<ItemProxy> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;

        if !item_ops::item_has_key(item, key)? {
            item_ops::set_with_decor_preservation(item, key, default);
        }

        Ok(self.child_proxy(py, Key::Str(key.to_owned())))
    }

    // ---- list-like methods ----

    #[pyo3(signature = (value, /))]
    pub fn append(&mut self, py: Python<'_>, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_append(item, value)?;
        Ok(())
    }

    #[pyo3(signature = (index, value, /))]
    pub fn insert(&mut self, py: Python<'_>, index: i64, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_insert(item, index, value)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn remove(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_remove(item, value)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    #[pyo3(signature = (values, /))]
    pub fn extend(&mut self, py: Python<'_>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let items: Vec<Item> = values
            .try_iter()?
            .map(|r| r.and_then(|v| v.extract::<Item>()))
            .collect::<PyResult<_>>()?;

        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_extend(item, items, "extend()")?;
        Ok(())
    }

    #[pyo3(signature = (value, /))]
    pub fn count(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_count(item, value)
    }

    #[pyo3(signature = (value, start=None, stop=None, /))]
    pub fn index(
        &self,
        py: Python<'_>,
        value: &Bound<'_, PyAny>,
        start: Option<i64>,
        stop: Option<i64>,
    ) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_ops::item_index(item, value, start, stop)
    }

    // ---- shared methods ----

    pub fn clear(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_clear(item)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    /// Normalize formatting of this item (spacing, trailing commas, etc.).
    ///
    /// Useful after mutations that leave behind awkward whitespace.
    /// This is shallow - it formats the item itself, not nested sub-tables.
    /// Note: any comments on the formatted item will be removed.
    pub fn fmt(&self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_ops::item_fmt(item);
        Ok(())
    }

    /// Parse a TOML value fragment, preserving its representation.
    ///
    /// Use this when you need a specific TOML representation that can't be
    /// expressed through plain Python types, e.g. hex integers or literal strings:
    ///
    ///     doc["mask"] = Item.parse("0xFF")
    ///     doc["msg"]  = Item.parse("'''multi\nline'''")
    #[staticmethod]
    fn parse(py: Python<'_>, text: &str) -> PyResult<Self> {
        let value: ValueRs = text
            .parse()
            .map_err(|e: toml_edit::TomlError| PyValueError::new_err(e.to_string()))?;
        let mut doc_rs = DocumentRs::new();
        doc_rs["_"] = ItemRs::Value(value);
        let doc = Py::new(
            py,
            Document {
                inner: doc_rs,
                generation: 0,
            },
        )?;
        let generation = 0;
        Ok(Self {
            document: doc,
            path: vec![Key::Str("_".to_owned())],
            generation,
        })
    }
}
