use pyo3::exceptions::{PyIndexError, PyKeyError, PyRuntimeError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{
    PyDate, PyDateTime, PyDelta, PyDict, PyIterator, PyList, PySlice, PyTime, PyTzInfo,
};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::document::Document;
use crate::equality;
use crate::item::Item;

// ---------------------------------------------------------------------------
// Key / proxy types
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub(crate) enum Key {
    Str(String),
    Int(usize),
}

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

/// A proxy into a Document that supports chained `__getitem__` / `__setitem__`.
///
/// Instead of cloning the underlying item (which breaks `doc["d"][0] = 7`),
/// ItemProxy holds a reference to the owning Document and a path of keys.
/// Reads and writes navigate that path at call-time so mutations are visible.
///
/// Each proxy snapshots the document's generation counter at creation time.
/// If the document is mutated through a different path, the proxy detects
/// the stale generation and raises RuntimeError on the next access.
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
    pub(crate) fn clone_item(&self, py: Python<'_>) -> PyResult<ItemRs> {
        let doc = self.document.borrow(py);
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item.clone())
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
            let len = require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = collect_slice_indices(si.start, si.stop, si.step);
            let proxies: Vec<ItemProxy> = indices
                .into_iter()
                .map(|i| self.child_proxy(py, Key::Int(i)))
                .collect();
            return Ok(proxies.into_pyobject(py)?.into_any().unbind());
        }

        let new_key = if let Ok(k) = key.extract::<i64>() {
            let doc = self.document.bind(py).borrow();
            self.check_generation(&doc)?;
            let item = self.navigate(&doc.inner)?;

            // Tables use string keys; only arrays support positional indexing.
            if item.is_table() || item.is_inline_table() {
                return Err(PyTypeError::new_err(
                    "TOML table keys must be strings, not integers",
                ));
            }

            let len = item_len(item).ok_or_else(|| {
                PyTypeError::new_err(format!(
                    "TOML {} item is not subscriptable (use .value to get the Python object)",
                    item.type_name()
                ))
            })?;
            Key::Int(resolve_index(k, len)?)
        } else if let Ok(k) = key.extract::<String>() {
            Key::Str(k)
        } else {
            return Err(bad_key_type(key));
        };

        {
            let doc = self.document.bind(py).borrow();
            self.check_generation(&doc)?;
            let item = self.navigate(&doc.inner)?;
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
            let len = require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            item_setitem_slice(item, si.start, si.stop, si.step, values)?;
            self.bump_generation(&mut doc);
            return Ok(());
        }

        let value: Item = value.extract()?;
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        let replaced = item_setitem(item, key, value)?;
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
            let len = require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = collect_slice_indices(si.start, si.stop, si.step);
            item_delitem_slice(item, &indices)?;
            self.bump_generation(&mut doc);
            return Ok(());
        }

        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_delitem(item, key)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    pub fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_len(item).ok_or_else(|| {
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

        match item_iter_info(item)? {
            IterKind::TableKeys(keys) => {
                let list = keys.into_pyobject(py)?;
                Ok(list.try_iter()?.unbind())
            }
            IterKind::ArrayLen(len) => {
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
        item_contains(item, value)
    }

    pub fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_bool(item))
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_str(item, py)
    }

    pub fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        Ok(item_repr(item))
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

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        item_to_py(item, py)
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

    #[setter]
    pub fn set_inline_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        if let Some(Key::Int(idx)) = self.path.last() {
            comments::set_array_item_comment(
                self.navigate_parent_mut(&mut doc.inner)?,
                *idx,
                value,
            )?;
            return Ok(());
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
        item_keys(item)
    }

    pub fn values(&self, py: Python<'_>) -> PyResult<Vec<ItemProxy>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        let keys = item_keys(item)?;
        Ok(keys
            .into_iter()
            .map(|k| self.child_proxy(py, Key::Str(k)))
            .collect())
    }

    pub fn items(&self, py: Python<'_>) -> PyResult<Vec<(String, ItemProxy)>> {
        let doc = self.document.bind(py).borrow();
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        let keys = item_keys(item)?;
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
        self.check_generation(&doc)?;
        let item = self.navigate(&doc.inner)?;
        if item_has_key(item, key)? {
            Ok(self
                .child_proxy(py, Key::Str(key.to_owned()))
                .into_pyobject(py)?
                .into_any()
                .unbind())
        } else {
            Ok(default.map_or_else(|| py.None(), |d| d.clone().unbind()))
        }
    }

    #[pyo3(signature = (key=None, default=None))]
    pub fn pop(
        &mut self,
        py: Python<'_>,
        key: Option<&Bound<'_, PyAny>>,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        match item_pop(item, key) {
            Ok(removed) => {
                let result = item_to_py(&removed.0, py)?;
                self.bump_generation(&mut doc);
                Ok(result)
            }
            Err(e) if default.is_some() && e.is_instance_of::<PyKeyError>(py) => {
                Ok(default.unwrap())
            }
            Err(e) => Err(e),
        }
    }

    pub fn update(&mut self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = other.py();
        let pairs = extract_update_pairs(other)?;
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        apply_update_pairs(item, pairs)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    pub fn setdefault(&mut self, py: Python<'_>, key: &str, default: Item) -> PyResult<ItemProxy> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;

        if !item_has_key(item, key)? {
            set_with_decor_preservation(item, key, default);
        }

        Ok(self.child_proxy(py, Key::Str(key.to_owned())))
    }

    // ---- list-like methods ----

    pub fn append(&mut self, py: Python<'_>, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_append(item, value)?;
        Ok(())
    }

    pub fn insert(&mut self, py: Python<'_>, index: i64, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_insert(item, index, value)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    pub fn remove(&mut self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_remove(item, value)?;
        self.bump_generation(&mut doc);
        Ok(())
    }

    pub fn extend(&mut self, py: Python<'_>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let items: Vec<Item> = values
            .try_iter()?
            .map(|r| r.and_then(|v| v.extract::<Item>()))
            .collect::<PyResult<_>>()?;

        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_extend(item, items)?;
        Ok(())
    }

    // ---- shared methods ----

    pub fn clear(&mut self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        self.check_generation(&doc)?;
        let item = self.navigate_mut(&mut doc.inner)?;
        item_clear(item)?;
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
        item_fmt(item);
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

// ===========================================================================
// Item operations
// ===========================================================================

// ---------------------------------------------------------------------------
// Decor preservation
// ---------------------------------------------------------------------------

pub(crate) fn set_with_decor_preservation(item: &mut ItemRs, key: &str, value: Item) {
    let old_decor = item
        .get(key)
        .and_then(|e| e.as_value())
        .map(|v| v.decor().clone());
    match (old_decor, value.0.into_value()) {
        (Some(decor), Ok(mut new_value)) => {
            if let Some(prefix) = decor.prefix() {
                new_value.decor_mut().set_prefix(prefix.clone());
            }
            if let Some(suffix) = decor.suffix() {
                new_value.decor_mut().set_suffix(suffix.clone());
            }
            item[key] = ItemRs::Value(new_value);
        }
        (_, Ok(new_value)) => {
            item[key] = ItemRs::Value(new_value);
        }
        (_, Err(new_item)) => {
            item[key] = new_item;
        }
    }
}

// ---------------------------------------------------------------------------
// Read operations
// ---------------------------------------------------------------------------

fn item_len(item: &ItemRs) -> Option<usize> {
    match item {
        ItemRs::Table(t) => Some(t.len()),
        ItemRs::Value(ValueRs::Array(a)) => Some(a.len()),
        ItemRs::Value(ValueRs::InlineTable(it)) => Some(it.len()),
        ItemRs::ArrayOfTables(aot) => Some(aot.len()),
        _ => None,
    }
}

fn item_contains(item: &ItemRs, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    match item {
        ItemRs::Table(table) => {
            let key: &str = value.extract()?;
            Ok(table.contains_key(key))
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let key: &str = value.extract()?;
            Ok(it.contains_key(key))
        }
        ItemRs::Value(ValueRs::Array(arr)) => {
            for v in arr.iter() {
                if equality::value_eq(v, value)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        ItemRs::ArrayOfTables(aot) => {
            if let Ok(other_dict) = value.cast::<PyDict>() {
                for table in aot.iter() {
                    if equality::table_entries_eq(table.iter(), table.len(), other_dict)? {
                        return Ok(true);
                    }
                }
            }
            Ok(false)
        }
        _ => Err(PyTypeError::new_err(
            "TOML scalar item does not support 'in' (use .value to get the Python object)",
        )),
    }
}

fn item_bool(item: &ItemRs) -> bool {
    if let Some(len) = item_len(item) {
        return len > 0;
    }
    // Scalar truthiness: match Python semantics.
    if let ItemRs::Value(value) = item {
        match value {
            ValueRs::Boolean(b) => *b.value(),
            ValueRs::Integer(i) => *i.value() != 0,
            ValueRs::Float(f) => *f.value() != 0.0,
            ValueRs::String(s) => !s.value().is_empty(),
            _ => true,
        }
    } else {
        true
    }
}

fn item_repr(item: &ItemRs) -> String {
    let type_name = item.type_name();
    let content = item.to_string();
    let trimmed = content.trim();
    format!("Item({type_name}, {trimmed})")
}

fn item_str(item: &ItemRs, py: Python<'_>) -> PyResult<String> {
    let obj = item_to_py(item, py)?;
    obj.call_method0(py, "__str__")?.extract::<String>(py)
}

/// Convert a toml_edit table's entries to a Python dict.
fn table_to_pydict<'a>(
    iter: impl Iterator<Item = (&'a str, &'a ItemRs)>,
    py: Python<'_>,
) -> PyResult<Bound<'_, PyDict>> {
    let dict = PyDict::new(py);
    for (k, v) in iter {
        dict.set_item(k, item_to_py(v, py)?)?;
    }
    Ok(dict)
}

/// Convert a toml_edit Item to a native Python object (dict/list/str/int/etc).
pub(crate) fn item_to_py(item: &ItemRs, py: Python<'_>) -> PyResult<Py<PyAny>> {
    match item {
        ItemRs::Value(v) => value_to_py(v, py),
        ItemRs::Table(table) => Ok(table_to_pydict(table.iter(), py)?.into_any().unbind()),
        ItemRs::ArrayOfTables(aot) => {
            let list = PyList::empty(py);
            for table in aot.iter() {
                list.append(table_to_pydict(table.iter(), py)?)?;
            }
            Ok(list.into_any().unbind())
        }
        _ => Ok(py.None()),
    }
}

fn value_to_py(value: &ValueRs, py: Python<'_>) -> PyResult<Py<PyAny>> {
    if let Some(s) = value.as_str() {
        return Ok(s.into_pyobject(py)?.into_any().unbind());
    }
    if let Some(i) = value.as_integer() {
        return Ok(i.into_pyobject(py)?.into_any().unbind());
    }
    if let Some(f) = value.as_float() {
        return Ok(f.into_pyobject(py)?.into_any().unbind());
    }
    if let Some(b) = value.as_bool() {
        return Ok(b.into_pyobject(py)?.to_owned().into_any().unbind());
    }
    if let Some(arr) = value.as_array() {
        let list = PyList::empty(py);
        for v in arr.iter() {
            list.append(value_to_py(v, py)?)?;
        }
        return Ok(list.into_any().unbind());
    }
    if let Some(it) = value.as_inline_table() {
        let dict = PyDict::new(py);
        for (k, v) in it.iter() {
            dict.set_item(k, value_to_py(v, py)?)?;
        }
        return Ok(dict.into_any().unbind());
    }
    if let Some(dt) = value.as_datetime() {
        return datetime_to_py(dt, py);
    }
    // Unreachable for valid TOML, but fall back to string representation.
    Ok(value
        .to_string()
        .trim()
        .into_pyobject(py)?
        .into_any()
        .unbind())
}

/// Convert a toml_edit Datetime to a Python datetime.datetime, date, or time.
fn datetime_to_py(dt: &toml_edit::Datetime, py: Python<'_>) -> PyResult<Py<PyAny>> {
    let make_tz = |offset: &toml_edit::Offset| -> PyResult<Bound<'_, PyTzInfo>> {
        let minutes: i32 = match offset {
            toml_edit::Offset::Z => 0,
            toml_edit::Offset::Custom { minutes } => *minutes as i32,
        };
        let td = PyDelta::new(py, 0, minutes * 60, 0, true)?;
        let datetime_mod = py.import("datetime")?;
        let tz = datetime_mod.getattr("timezone")?.call1((&td,))?;
        Ok(tz.cast::<PyTzInfo>()?.to_owned())
    };

    match (&dt.date, &dt.time) {
        (Some(date), Some(time)) => {
            let tzinfo = dt.offset.as_ref().map(make_tz).transpose()?;
            Ok(PyDateTime::new(
                py,
                date.year.into(),
                date.month,
                date.day,
                time.hour,
                time.minute,
                time.second.unwrap_or(0),
                time.nanosecond.unwrap_or(0) / 1000,
                tzinfo.as_ref(),
            )?
            .into_any()
            .unbind())
        }
        (Some(date), None) => Ok(PyDate::new(py, date.year.into(), date.month, date.day)?
            .into_any()
            .unbind()),
        (None, Some(time)) => Ok(PyTime::new(
            py,
            time.hour,
            time.minute,
            time.second.unwrap_or(0),
            time.nanosecond.unwrap_or(0) / 1000,
            None,
        )?
        .into_any()
        .unbind()),
        (None, None) => Ok(dt.to_string().into_pyobject(py)?.into_any().unbind()),
    }
}

/// Return the number of iterable children, or a TypeError for scalars.
enum IterKind<'a> {
    TableKeys(Vec<&'a str>),
    ArrayLen(usize),
}

fn item_iter_info<'a>(item: &'a ItemRs) -> PyResult<IterKind<'a>> {
    match item {
        ItemRs::Table(table) => Ok(IterKind::TableKeys(table.iter().map(|(k, _)| k).collect())),
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            Ok(IterKind::TableKeys(it.iter().map(|(k, _)| k).collect()))
        }
        ItemRs::Value(ValueRs::Array(arr)) => Ok(IterKind::ArrayLen(arr.len())),
        ItemRs::ArrayOfTables(aot) => Ok(IterKind::ArrayLen(aot.len())),
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item is not iterable (use .value to get the Python object)",
            item.type_name()
        ))),
    }
}

/// Resolve a Python index (possibly negative) against a known length.
fn resolve_index(index: i64, len: usize) -> PyResult<usize> {
    let resolved = if index < 0 { len as i64 + index } else { index };
    if resolved < 0 || resolved as usize >= len {
        Err(PyIndexError::new_err("index out of range"))
    } else {
        Ok(resolved as usize)
    }
}

// ---------------------------------------------------------------------------
// Slice support
// ---------------------------------------------------------------------------

/// Collect concrete indices from resolved slice parameters.
fn collect_slice_indices(start: isize, stop: isize, step: isize) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut i = start;
    if step > 0 {
        while i < stop {
            indices.push(i as usize);
            i += step;
        }
    } else if step < 0 {
        while i > stop {
            indices.push(i as usize);
            i += step;
        }
    }
    indices
}

/// Get the length of an array-like item, or error for non-sliceable types.
fn require_array_like_len(item: &ItemRs) -> PyResult<usize> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => Ok(arr.len()),
        ItemRs::ArrayOfTables(aot) => Ok(aot.len()),
        _ => Err(unsupported_op(item, "slicing")),
    }
}

/// Delete elements at the given indices (sorted in reverse internally).
fn item_delitem_slice(item: &mut ItemRs, indices: &[usize]) -> PyResult<()> {
    let mut sorted = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.reverse();

    match item {
        ItemRs::Value(ValueRs::Array(arr)) => {
            for idx in sorted {
                arr.remove(idx);
            }
            Ok(())
        }
        ItemRs::ArrayOfTables(aot) => {
            for idx in sorted {
                aot.remove(idx);
            }
            Ok(())
        }
        _ => Err(unsupported_op(item, "slice deletion")),
    }
}

/// Assign to a slice of an array.
fn item_setitem_slice(
    item: &mut ItemRs,
    start: isize,
    stop: isize,
    step: isize,
    values: Vec<Item>,
) -> PyResult<()> {
    let Some(arr) = item.as_array_mut() else {
        return Err(PyTypeError::new_err(format!(
            "'{}' does not support slice assignment",
            item.type_name()
        )));
    };

    if step == 1 {
        // Contiguous slice: replacement can be a different length.
        let start_idx = start as usize;
        let stop_idx = stop as usize;

        // Remove old elements from back to front.
        for i in (start_idx..stop_idx).rev() {
            arr.remove(i);
        }

        // Insert new elements at start position.
        for (offset, value) in values.into_iter().enumerate() {
            let v = into_value(value)?;
            let idx = start_idx + offset;
            if idx >= arr.len() {
                arr.push(v);
            } else {
                arr.insert(idx, v);
            }
        }
        Ok(())
    } else {
        // Extended slice: replacement must match the slice length.
        let indices = collect_slice_indices(start, stop, step);
        if indices.len() != values.len() {
            return Err(PyValueError::new_err(format!(
                "attempt to assign sequence of size {} to extended slice of size {}",
                values.len(),
                indices.len()
            )));
        }
        for (idx, value) in indices.into_iter().zip(values) {
            let v = into_value(value)?;
            arr.replace(idx, v);
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Misc helpers
// ---------------------------------------------------------------------------

fn bad_key_type(key: &Bound<'_, PyAny>) -> PyErr {
    let type_name = key
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "?".to_owned());
    PyTypeError::new_err(format!(
        "indices must be integers or strings, not {type_name}"
    ))
}

fn unsupported_op(item: &ItemRs, op: &str) -> PyErr {
    PyTypeError::new_err(format!(
        "TOML {} item does not support {op}",
        item.type_name()
    ))
}

fn into_value(item: Item) -> PyResult<ValueRs> {
    item.0.into_value().map_err(|item| {
        PyTypeError::new_err(format!(
            "cannot convert {} to a TOML value",
            item.type_name()
        ))
    })
}

fn item_keys(item: &ItemRs) -> PyResult<Vec<String>> {
    match item {
        ItemRs::Table(table) => Ok(table.iter().map(|(k, _)| k.to_owned()).collect()),
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            Ok(it.iter().map(|(k, _)| k.to_owned()).collect())
        }
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item has no keys()",
            item.type_name()
        ))),
    }
}

fn item_has_key(item: &ItemRs, key: &str) -> PyResult<bool> {
    match item {
        ItemRs::Table(table) => Ok(table.contains_key(key)),
        ItemRs::Value(ValueRs::InlineTable(it)) => Ok(it.contains_key(key)),
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item has no get()",
            item.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Setitem
// ---------------------------------------------------------------------------

/// Returns `true` if an existing value was replaced, `false` if a new key was added.
fn item_setitem(item: &mut ItemRs, key: &Bound<'_, PyAny>, value: Item) -> PyResult<bool> {
    match item {
        ItemRs::Table(t) => {
            let key: &str = key.extract()?;
            let replaced = t.contains_key(key);
            set_with_decor_preservation(item, key, value);
            Ok(replaced)
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let key: &str = key.extract()?;
            let replaced = it.contains_key(key);
            set_with_decor_preservation(item, key, value);
            Ok(replaced)
        }
        ItemRs::Value(ValueRs::Array(array)) => {
            let v = into_value(value)?;
            let idx = resolve_index(key.extract::<i64>()?, array.len())?;
            array.replace(idx, v);
            Ok(true)
        }
        ItemRs::ArrayOfTables(aot) => {
            let idx = resolve_index(key.extract::<i64>()?, aot.len())?;
            item[idx] = value.0;
            Ok(true)
        }
        _ => Err(PyTypeError::new_err(format!(
            "'{}' is not subscriptable",
            item.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Delitem
// ---------------------------------------------------------------------------

fn item_delitem(item: &mut ItemRs, key: &Bound<'_, PyAny>) -> PyResult<()> {
    match item {
        ItemRs::Table(table) => {
            let key: &str = key.extract()?;
            if table.remove(key).is_none() {
                return Err(PyKeyError::new_err(key.to_owned()));
            }
            Ok(())
        }
        ItemRs::Value(value_rs) => match value_rs {
            ValueRs::Array(array) => {
                let idx = resolve_index(key.extract::<i64>()?, array.len())?;
                array.remove(idx);
                Ok(())
            }
            ValueRs::InlineTable(inline_table) => {
                let key: &str = key.extract()?;
                if inline_table.remove(key).is_none() {
                    return Err(PyKeyError::new_err(key.to_owned()));
                }
                Ok(())
            }
            _ => Err(PyTypeError::new_err(
                "TOML scalar item is not subscriptable",
            )),
        },
        ItemRs::ArrayOfTables(aot) => {
            let idx = resolve_index(key.extract::<i64>()?, aot.len())?;
            aot.remove(idx);
            Ok(())
        }
        _ => Err(PyTypeError::new_err("TOML item is not subscriptable")),
    }
}

// ---------------------------------------------------------------------------
// Mutation: dict-like
// ---------------------------------------------------------------------------

fn item_pop(item: &mut ItemRs, key: Option<&Bound<'_, PyAny>>) -> PyResult<Item> {
    match key {
        Some(key_obj) => match item {
            ItemRs::Table(table) => {
                let key: &str = key_obj.extract()?;
                table
                    .remove(key)
                    .map(Item)
                    .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
            }
            ItemRs::Value(ValueRs::InlineTable(it)) => {
                let key: &str = key_obj.extract()?;
                it.remove(key)
                    .map(|v| Item(ItemRs::Value(v)))
                    .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
            }
            ItemRs::Value(ValueRs::Array(arr)) => {
                let idx = resolve_index(key_obj.extract::<i64>()?, arr.len())?;
                Ok(Item(ItemRs::Value(arr.remove(idx))))
            }
            _ => Err(unsupported_op(item, "pop()")),
        },
        None => match item {
            ItemRs::Value(ValueRs::Array(arr)) => {
                if arr.is_empty() {
                    return Err(PyIndexError::new_err("pop from empty array"));
                }
                let last = arr.len() - 1;
                Ok(Item(ItemRs::Value(arr.remove(last))))
            }
            _ => Err(PyTypeError::new_err(
                "pop() with no argument is only supported on arrays",
            )),
        },
    }
}

/// Extract key-value pairs from a dict for update(), before borrowing the document.
pub(crate) fn extract_update_pairs(other: &Bound<'_, PyAny>) -> PyResult<Vec<(String, Item)>> {
    let dict = other
        .cast::<PyDict>()
        .map_err(|_| PyTypeError::new_err("update() argument must be a dict"))?;
    let mut pairs = Vec::with_capacity(dict.len());
    for (k, v) in dict.iter() {
        let key: String = k.extract()?;
        let val: Item = v.extract()?;
        pairs.push((key, val));
    }
    Ok(pairs)
}

/// Apply pre-extracted update pairs to an item.
pub(crate) fn apply_update_pairs(item: &mut ItemRs, pairs: Vec<(String, Item)>) -> PyResult<()> {
    if !(item.is_table() || item.is_inline_table()) {
        return Err(unsupported_op(item, "update()"));
    }
    for (key, val) in pairs {
        set_with_decor_preservation(item, &key, val);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mutation: list-like
// ---------------------------------------------------------------------------

fn item_append(item: &mut ItemRs, value: Item) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        let v = into_value(value)?;
        arr.push(v);
        Ok(())
    } else {
        Err(unsupported_op(item, "append()"))
    }
}

fn item_insert(item: &mut ItemRs, index: i64, value: Item) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        let len = arr.len();
        // Clamp like Python's list.insert: negative wraps, out-of-range clamps.
        let resolved = if index < 0 {
            (len as i64 + index).max(0) as usize
        } else {
            (index as usize).min(len)
        };
        let v = into_value(value)?;
        arr.insert(resolved, v);
        Ok(())
    } else {
        Err(unsupported_op(item, "insert()"))
    }
}

fn item_remove(item: &mut ItemRs, value: &Bound<'_, PyAny>) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        for i in 0..arr.len() {
            if let Some(v) = arr.get(i)
                && equality::value_eq(v, value)?
            {
                arr.remove(i);
                return Ok(());
            }
        }
        Err(PyValueError::new_err("value not in array"))
    } else {
        Err(unsupported_op(item, "remove()"))
    }
}

fn item_extend(item: &mut ItemRs, items: Vec<Item>) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        for new_item in items {
            let v = into_value(new_item)?;
            arr.push(v);
        }
        Ok(())
    } else {
        Err(unsupported_op(item, "extend()"))
    }
}

fn item_clear(item: &mut ItemRs) -> PyResult<()> {
    match item {
        ItemRs::Table(table) => {
            table.clear();
            Ok(())
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            it.clear();
            Ok(())
        }
        ItemRs::Value(ValueRs::Array(arr)) => {
            arr.clear();
            Ok(())
        }
        ItemRs::ArrayOfTables(aot) => {
            aot.clear();
            Ok(())
        }
        _ => Err(unsupported_op(item, "clear()")),
    }
}

/// Normalize formatting of a single item (shallow).
fn item_fmt(item: &mut ItemRs) {
    match item {
        ItemRs::Table(table) => table.fmt(),
        ItemRs::Value(ValueRs::InlineTable(it)) => it.fmt(),
        ItemRs::Value(ValueRs::Array(arr)) => arr.fmt(),
        _ => {} // ArrayOfTables, scalars: no-op
    }
}
