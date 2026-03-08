use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{
    PyDate, PyDateTime, PyDelta, PyDict, PyIterator, PyList, PySlice, PyTime, PyTzInfo,
};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;

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

    /// Navigate to the parent item (all path segments except the last).
    fn navigate_parent<'a>(&self, doc: &'a DocumentRs) -> PyResult<&'a ItemRs> {
        let mut current: &ItemRs = doc.as_item();
        for key in &self.path[..self.path.len() - 1] {
            let next = match key {
                Key::Str(s) => current.get(s.as_str()),
                Key::Int(i) => current.get(*i),
            };
            current = next.ok_or_else(|| PyKeyError::new_err("path no longer valid"))?;
        }
        Ok(current)
    }

    fn navigate_parent_mut<'a>(&self, doc: &'a mut DocumentRs) -> PyResult<&'a mut ItemRs> {
        let mut current: &mut ItemRs = doc.as_item_mut();
        for key in &self.path[..self.path.len() - 1] {
            let next = match key {
                Key::Str(s) => current.get_mut(s.as_str()),
                Key::Int(i) => current.get_mut(*i),
            };
            current = next.ok_or_else(|| PyKeyError::new_err("path no longer valid"))?;
        }
        Ok(current)
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
            // Resolve negative indices
            let doc = self.document.bind(py).borrow();
            let item = self.navigate(&doc.0)?;
            let len = item_len(item).ok_or_else(|| {
                PyTypeError::new_err(format!("'{}' is not subscriptable", item.type_name()))
            })?;
            Key::Int(resolve_index(k, len)?)
        } else if let Ok(k) = key.extract::<String>() {
            Key::Str(k)
        } else {
            return Err(bad_key_type(key));
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
            let len = require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let values: Vec<Item> = value
                .try_iter()?
                .map(|r| r.and_then(|v| v.extract::<Item>()))
                .collect::<PyResult<_>>()?;
            return item_setitem_slice(item, si.start, si.stop, si.step, values);
        }

        let value: Item = value.extract()?;
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_setitem(item, key, value)
    }

    pub fn __delitem__(&self, key: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = key.py();

        if let Ok(slice) = key.cast::<PySlice>() {
            let mut doc = self.document.bind(py).borrow_mut();
            let item = self.navigate_mut(&mut doc.0)?;
            let len = require_array_like_len(item)?;
            let si = slice.indices(len as isize)?;
            let indices = collect_slice_indices(si.start, si.stop, si.step);
            return item_delitem_slice(item, &indices);
        }

        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_delitem(item, key)
    }

    pub fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        item_len(item)
            .ok_or_else(|| PyTypeError::new_err(format!("'{}' has no len()", item.type_name())))
    }

    pub fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyIterator>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;

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
        let item = self.navigate(&doc.0)?;
        item_contains(item, value)
    }

    pub fn __bool__(&self, py: Python<'_>) -> PyResult<bool> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        Ok(item_bool(item))
    }

    pub fn __str__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        item_str(item, py)
    }

    pub fn __repr__(&self, py: Python<'_>) -> PyResult<String> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        Ok(item_repr(item))
    }

    pub fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let py = other.py();

        // Proxy-vs-proxy: compare underlying items directly in Rust.
        if let Ok(other_proxy) = other.cast::<Self>() {
            let other_proxy = other_proxy.borrow();
            let doc = self.document.bind(py).borrow();
            let self_item = self.navigate(&doc.0)?;
            let other_doc = other_proxy.document.bind(py).borrow();
            let other_item = other_proxy.navigate(&other_doc.0)?;
            return Ok(equality::items_structural_eq(self_item, other_item));
        }

        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        equality::item_eq(item, other)
    }

    /// The underlying data as a native Python object (int, str, list, dict, etc).
    #[getter]
    pub fn value(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        item_to_py(item, py)
    }

    // ---- comment access ----

    /// The comment lines before this entry, or None.
    #[getter]
    pub fn comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        if self.path.is_empty() {
            return Ok(None);
        }
        let doc = self.document.bind(py).borrow();
        let last_key = self.path.last().unwrap();
        match last_key {
            Key::Str(key_str) => {
                let parent = self.navigate_parent(&doc.0)?;
                Ok(comments::get_key_prefix_comment(parent, key_str))
            }
            Key::Int(_) => {
                let item = self.navigate(&doc.0)?;
                Ok(comments::get_value_prefix_comment(item))
            }
        }
    }

    #[setter]
    pub fn set_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        if self.path.is_empty() {
            return Err(PyTypeError::new_err("cannot set comment on root"));
        }
        let last_key = self.path.last().unwrap().clone();
        let mut doc = self.document.bind(py).borrow_mut();
        match last_key {
            Key::Str(key_str) => {
                let parent = self.navigate_parent_mut(&mut doc.0)?;
                comments::set_key_prefix_comment(parent, &key_str, value)
            }
            Key::Int(_) => {
                let item = self.navigate_mut(&mut doc.0)?;
                comments::set_value_prefix_comment(item, value)
            }
        }
    }

    /// The inline comment after this value (e.g. `key = 1 # this part`), or None.
    #[getter]
    pub fn inline_comment(&self, py: Python<'_>) -> PyResult<Option<String>> {
        let doc = self.document.bind(py).borrow();
        if let Some(Key::Int(idx)) = self.path.last() {
            let parent = self.navigate_parent(&doc.0)?;
            return Ok(comments::get_array_item_comment(parent, *idx));
        }
        let item = self.navigate(&doc.0)?;
        Ok(comments::get_suffix_comment(item))
    }

    #[setter]
    pub fn set_inline_comment(&self, py: Python<'_>, value: Option<&str>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        if let Some(Key::Int(idx)) = self.path.last() {
            return comments::set_array_item_comment(
                self.navigate_parent_mut(&mut doc.0)?,
                *idx,
                value,
            );
        }
        let item = self.navigate_mut(&mut doc.0)?;
        comments::set_suffix_comment(item, value)
    }

    // ---- dict-like methods ----

    pub fn keys(&self, py: Python<'_>) -> PyResult<Vec<String>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        item_keys(item)
    }

    pub fn values(&self, py: Python<'_>) -> PyResult<Vec<ItemProxy>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
        let keys = item_keys(item)?;
        Ok(keys
            .into_iter()
            .map(|k| self.child_proxy(py, Key::Str(k)))
            .collect())
    }

    pub fn items(&self, py: Python<'_>) -> PyResult<Vec<(String, ItemProxy)>> {
        let doc = self.document.bind(py).borrow();
        let item = self.navigate(&doc.0)?;
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
        let item = self.navigate(&doc.0)?;
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
        &self,
        py: Python<'_>,
        key: Option<&Bound<'_, PyAny>>,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        match item_pop(item, key) {
            Ok(removed) => item_to_py(&removed.0, py),
            Err(e) if default.is_some() && e.is_instance_of::<PyKeyError>(py) => {
                Ok(default.unwrap())
            }
            Err(e) => Err(e),
        }
    }

    pub fn update(&self, other: &Bound<'_, PyAny>) -> PyResult<()> {
        let py = other.py();
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_update(item, other)
    }

    pub fn setdefault(&self, py: Python<'_>, key: &str, default: Item) -> PyResult<ItemProxy> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;

        if !item_has_key(item, key)? {
            set_with_decor_preservation(item, key, default);
        }

        drop(doc);
        Ok(self.child_proxy(py, Key::Str(key.to_owned())))
    }

    // ---- list-like methods ----

    pub fn append(&self, py: Python<'_>, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_append(item, value)
    }

    pub fn insert(&self, py: Python<'_>, index: i64, value: Item) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_insert(item, index, value)
    }

    pub fn remove(&self, py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_remove(item, value)
    }

    pub fn extend(&self, py: Python<'_>, values: &Bound<'_, PyAny>) -> PyResult<()> {
        let items: Vec<Item> = values
            .try_iter()?
            .map(|r| r.and_then(|v| v.extract::<Item>()))
            .collect::<PyResult<_>>()?;

        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_extend(item, items)
    }

    // ---- shared methods ----

    pub fn clear(&self, py: Python<'_>) -> PyResult<()> {
        let mut doc = self.document.bind(py).borrow_mut();
        let item = self.navigate_mut(&mut doc.0)?;
        item_clear(item)
    }
}

// ===========================================================================
// Item operations (formerly ops.rs)
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
        ItemRs::Value(toml_edit::Value::Array(a)) => Some(a.len()),
        ItemRs::Value(toml_edit::Value::InlineTable(it)) => Some(it.len()),
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
        ItemRs::Value(toml_edit::Value::InlineTable(it)) => {
            let key: &str = value.extract()?;
            Ok(it.contains_key(key))
        }
        ItemRs::Value(toml_edit::Value::Array(arr)) => {
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
            "argument of type 'scalar' is not iterable",
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
            toml_edit::Value::Boolean(b) => *b.value(),
            toml_edit::Value::Integer(i) => *i.value() != 0,
            toml_edit::Value::Float(f) => *f.value() != 0.0,
            toml_edit::Value::String(s) => !s.value().is_empty(),
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

fn value_to_py(value: &toml_edit::Value, py: Python<'_>) -> PyResult<Py<PyAny>> {
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
        ItemRs::Value(toml_edit::Value::InlineTable(it)) => {
            Ok(IterKind::TableKeys(it.iter().map(|(k, _)| k).collect()))
        }
        ItemRs::Value(toml_edit::Value::Array(arr)) => Ok(IterKind::ArrayLen(arr.len())),
        ItemRs::ArrayOfTables(aot) => Ok(IterKind::ArrayLen(aot.len())),
        _ => Err(PyTypeError::new_err(format!(
            "'{}' is not iterable",
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
    if let Some(arr) = item.as_array() {
        Ok(arr.len())
    } else if let Some(aot) = item.as_array_of_tables() {
        Ok(aot.len())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support slicing",
            item.type_name()
        )))
    }
}

/// Delete elements at the given indices (sorted in reverse internally).
fn item_delitem_slice(item: &mut ItemRs, indices: &[usize]) -> PyResult<()> {
    let mut sorted = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.reverse();

    if let Some(arr) = item.as_array_mut() {
        for idx in sorted {
            arr.remove(idx);
        }
        Ok(())
    } else if let Some(aot) = item.as_array_of_tables_mut() {
        for idx in sorted {
            aot.remove(idx);
        }
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support slice deletion",
            item.type_name()
        )))
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
            let v = value
                .0
                .into_value()
                .map_err(|_| PyTypeError::new_err("cannot assign non-value to array slice"))?;
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
            let v = value
                .0
                .into_value()
                .map_err(|_| PyTypeError::new_err("cannot assign non-value to array slice"))?;
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

fn item_keys(item: &ItemRs) -> PyResult<Vec<String>> {
    match item {
        ItemRs::Table(table) => Ok(table.iter().map(|(k, _)| k.to_owned()).collect()),
        ItemRs::Value(toml_edit::Value::InlineTable(it)) => {
            Ok(it.iter().map(|(k, _)| k.to_owned()).collect())
        }
        _ => Err(PyTypeError::new_err(format!(
            "'{}' has no keys()",
            item.type_name()
        ))),
    }
}

fn item_has_key(item: &ItemRs, key: &str) -> PyResult<bool> {
    match item {
        ItemRs::Table(table) => Ok(table.contains_key(key)),
        ItemRs::Value(toml_edit::Value::InlineTable(it)) => Ok(it.contains_key(key)),
        _ => Err(PyTypeError::new_err(format!(
            "'{}' has no get()",
            item.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Setitem
// ---------------------------------------------------------------------------

fn item_setitem(item: &mut ItemRs, key: &Bound<'_, PyAny>, value: Item) -> PyResult<()> {
    match item {
        ItemRs::Value(toml_edit::Value::Array(array)) => {
            let v = value
                .0
                .into_value()
                .map_err(|_| PyTypeError::new_err("Item cannot be assigned to array"))?;
            let idx = resolve_index(key.extract::<i64>()?, array.len())?;
            array.replace(idx, v);
            Ok(())
        }
        ItemRs::Value(toml_edit::Value::InlineTable(_)) | ItemRs::Table(_) => {
            let key: &str = key.extract()?;
            set_with_decor_preservation(item, key, value);
            Ok(())
        }
        ItemRs::Value(value_rs) => {
            let name = value_rs.type_name();
            Err(PyTypeError::new_err(format!(
                "'{name}' is not subscriptable"
            )))
        }
        ItemRs::ArrayOfTables(aot) => {
            let idx = resolve_index(key.extract::<i64>()?, aot.len())?;
            item[idx] = value.0;
            Ok(())
        }
        _ => {
            let name = item.type_name();
            Err(PyTypeError::new_err(format!(
                "'{name}' is not subscriptable"
            )))
        }
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
            toml_edit::Value::Array(array) => {
                let idx = resolve_index(key.extract::<i64>()?, array.len())?;
                array.remove(idx);
                Ok(())
            }
            toml_edit::Value::InlineTable(inline_table) => {
                let key: &str = key.extract()?;
                if inline_table.remove(key).is_none() {
                    return Err(PyKeyError::new_err(key.to_owned()));
                }
                Ok(())
            }
            _ => Err(PyTypeError::new_err("scalar value is not subscriptable")),
        },
        ItemRs::ArrayOfTables(aot) => {
            let idx = resolve_index(key.extract::<i64>()?, aot.len())?;
            aot.remove(idx);
            Ok(())
        }
        _ => Err(PyTypeError::new_err("item is not subscriptable")),
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
            ItemRs::Value(toml_edit::Value::InlineTable(it)) => {
                let key: &str = key_obj.extract()?;
                it.remove(key)
                    .map(|v| Item(ItemRs::Value(v)))
                    .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
            }
            ItemRs::Value(toml_edit::Value::Array(arr)) => {
                let idx = resolve_index(key_obj.extract::<i64>()?, arr.len())?;
                Ok(Item(ItemRs::Value(arr.remove(idx))))
            }
            _ => Err(PyTypeError::new_err(format!(
                "'{}' does not support pop()",
                item.type_name()
            ))),
        },
        None => match item {
            ItemRs::Value(toml_edit::Value::Array(arr)) => {
                if arr.is_empty() {
                    return Err(PyIndexError::new_err("pop from empty array"));
                }
                let last = arr.len() - 1;
                Ok(Item(ItemRs::Value(arr.remove(last))))
            }
            _ => Err(PyTypeError::new_err(
                "pop() with no arguments only supported on arrays",
            )),
        },
    }
}

pub(crate) fn item_update(item: &mut ItemRs, other: &Bound<'_, PyAny>) -> PyResult<()> {
    let dict = other
        .cast::<PyDict>()
        .map_err(|_| PyTypeError::new_err("update() argument must be a dict"))?;

    if !(item.is_table() || item.is_inline_table()) {
        return Err(PyTypeError::new_err(format!(
            "'{}' does not support update()",
            item.type_name()
        )));
    }

    for (k, v) in dict.iter() {
        let key: &str = k.extract()?;
        let val: Item = v.extract()?;
        set_with_decor_preservation(item, key, val);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Mutation: list-like
// ---------------------------------------------------------------------------

fn item_append(item: &mut ItemRs, value: Item) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        let v = value
            .0
            .into_value()
            .map_err(|_| PyTypeError::new_err("cannot append non-value to array"))?;
        arr.push(v);
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support append()",
            item.type_name()
        )))
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
        let v = value
            .0
            .into_value()
            .map_err(|_| PyTypeError::new_err("cannot insert non-value into array"))?;
        arr.insert(resolved, v);
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support insert()",
            item.type_name()
        )))
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
        Err(PyTypeError::new_err(format!(
            "'{}' does not support remove()",
            item.type_name()
        )))
    }
}

fn item_extend(item: &mut ItemRs, items: Vec<Item>) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        for new_item in items {
            let v = new_item
                .0
                .into_value()
                .map_err(|_| PyTypeError::new_err("cannot extend array with non-value"))?;
            arr.push(v);
        }
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support extend()",
            item.type_name()
        )))
    }
}

fn item_clear(item: &mut ItemRs) -> PyResult<()> {
    if let Some(table) = item.as_table_mut() {
        table.clear();
        Ok(())
    } else if let Some(it) = item.as_inline_table_mut() {
        it.clear();
        Ok(())
    } else if let Some(arr) = item.as_array_mut() {
        arr.clear();
        Ok(())
    } else if let Some(aot) = item.as_array_of_tables_mut() {
        aot.clear();
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support clear()",
            item.type_name()
        )))
    }
}
