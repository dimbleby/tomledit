use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyDict, PyList, PySlice, PyTime, PyTzInfo};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::dict_ops;
use crate::equality;
use crate::item::Item;
use crate::item_proxy::ItemProxy;
use crate::list_ops;

// ---------------------------------------------------------------------------
// Core types
// ---------------------------------------------------------------------------

/// A path component: string key for tables, integer index for arrays.
#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) enum Key {
    Str(String),
    Int(usize),
}

/// Describes how a mutation affects existing proxies, for invalidation.
pub(crate) enum Affected {
    /// Only a single child key was changed (replaced in place, or
    /// removed at the end of an array without shifting).
    Child(Key),
    /// Array indices from `from` up to (not including) `to` were
    /// shifted or removed.
    Range { from: usize, to: usize },
}

impl Affected {
    /// Compute invalidation for removing an element at `index` in an
    /// array of length `len` (measured *before* removal).
    pub(crate) fn for_removal(index: usize, len: usize) -> Self {
        if index + 1 == len {
            Self::Child(Key::Int(index))
        } else {
            Self::Range {
                from: index,
                to: len,
            }
        }
    }
}

/// Navigate a key path to an item within a document.
pub(crate) fn navigate_path<'a>(doc: &'a DocumentRs, path: &[Key]) -> PyResult<&'a ItemRs> {
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

/// Navigate a key path to a mutable item within a document.
pub(crate) fn navigate_path_mut<'a>(
    doc: &'a mut DocumentRs,
    path: &[Key],
) -> PyResult<&'a mut ItemRs> {
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

// ---------------------------------------------------------------------------
// Subscript key resolution
// ---------------------------------------------------------------------------

/// A subscript key resolved from Python *before* borrowing the document.
///
/// Proxy values are resolved to their plain Python equivalents so that
/// `extract()` works without re-borrowing the document.
pub(crate) enum SubscriptKey<'py> {
    Str(String),
    Int(i64),
    Slice(Bound<'py, PySlice>),
    /// Key type that is neither string, integer, nor slice.
    /// The callsite decides the error (e.g. `KeyError` for dicts, `TypeError`
    /// for arrays) because the correct exception depends on the item type.
    Other(Bound<'py, PyAny>),
}

/// Resolve a `__getitem__`/`__setitem__`/`__delitem__` key to a typed enum.
///
/// This must be called *before* borrowing the document, because resolving a
/// proxy key borrows it internally.
pub(crate) fn resolve_subscript_key<'py>(
    py: Python<'py>,
    key: &Bound<'py, PyAny>,
) -> PyResult<SubscriptKey<'py>> {
    if let Ok(slice) = key.cast::<PySlice>() {
        return Ok(SubscriptKey::Slice(slice.clone()));
    }
    // Resolve proxy to plain Python value before extracting.
    let resolved = crate::item_proxy::resolve_proxy(key)?;
    let key = resolved.as_ref().map_or(key, |v| v.bind(py));
    if let Ok(s) = key.extract::<String>() {
        Ok(SubscriptKey::Str(s))
    } else if let Ok(i) = key.extract::<i64>() {
        Ok(SubscriptKey::Int(i))
    } else {
        Ok(SubscriptKey::Other(key.clone()))
    }
}

// ---------------------------------------------------------------------------
// Error helpers
// ---------------------------------------------------------------------------

/// Error for a subscript key that is not a string, integer, or slice.
pub(crate) fn invalid_subscript_type(key: &Bound<'_, PyAny>) -> PyErr {
    let type_name = key
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "?".to_owned());
    PyTypeError::new_err(format!(
        "indices must be integers or strings, not {type_name}"
    ))
}

/// Error for a non-string/int subscript key, matching Python dict semantics:
/// KeyError for tables (like dict), TypeError for arrays.
pub(crate) fn invalid_subscript(key: &Bound<'_, PyAny>, item: &ItemRs) -> PyErr {
    match item {
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => key.repr().map_or_else(
            |_| PyKeyError::new_err("?"),
            |r| PyKeyError::new_err(r.to_string()),
        ),
        _ => invalid_subscript_type(key),
    }
}

/// Error for using the wrong key type on a subscriptable item.
/// Tables expect strings; arrays expect integers; scalars aren't subscriptable.
fn subscript_type_error(item: &ItemRs) -> PyErr {
    match item {
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => {
            PyTypeError::new_err("TOML table keys must be strings, not integers")
        }
        ItemRs::Value(ValueRs::Array(_)) | ItemRs::ArrayOfTables(_) => {
            PyTypeError::new_err("TOML array indices must be integers, not strings")
        }
        _ => PyTypeError::new_err(format!("'{}' is not subscriptable", item.type_name())),
    }
}

/// Error for an operation not supported by this item type.
pub(crate) fn unsupported_op(item: &ItemRs, op: &str) -> PyErr {
    PyTypeError::new_err(format!(
        "TOML {} item does not support {op}",
        item.type_name()
    ))
}

// ---------------------------------------------------------------------------
// Read operations
// ---------------------------------------------------------------------------

/// Extract a string suitable for use as a TOML key from `value`.
///
/// Fast path: `value` is a plain Python string.
/// Proxy path: `value` is an `ItemProxy` wrapping a TOML string value —
/// navigates the proxy in Rust to get the string without converting to Python.
/// Returns `Ok(None)` when the value is not (or does not wrap) a string.
/// Errors from stale or otherwise invalid proxies are preserved.
pub(crate) fn extract_key_str(value: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    if let Ok(s) = value.extract::<String>() {
        return Ok(Some(s));
    }
    let Ok(proxy) = value.cast::<ItemProxy>() else {
        return Ok(None);
    };
    let proxy = proxy.borrow();
    let doc = proxy.document.bind(value.py()).borrow();
    proxy.check_fresh(&doc)?;
    let item = proxy.navigate(&doc.inner)?;
    Ok(match item {
        ItemRs::Value(ValueRs::String(s)) => Some(s.value().to_owned()),
        _ => None,
    })
}

/// Get the length of a container item, or `None` for scalars.
pub(crate) fn item_len(item: &ItemRs) -> Option<usize> {
    match item {
        ItemRs::Table(t) => Some(t.len()),
        ItemRs::Value(ValueRs::Array(a)) => Some(a.len()),
        ItemRs::Value(ValueRs::InlineTable(it)) => Some(it.len()),
        ItemRs::ArrayOfTables(aot) => Some(aot.len()),
        _ => None,
    }
}

/// Test whether `value` is contained in `item` (tables check keys, arrays check elements).
pub(crate) fn item_contains(item: &ItemRs, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Some(tbl) = item.as_table_like() {
        let Some(key) = extract_key_str(value)? else {
            return Ok(false);
        };
        return Ok(tbl.contains_key(&key));
    }
    // For array containment, resolve proxies to plain Python values so that
    // value_eq / table_eq can compare without re-borrowing through dunders.
    let resolved = crate::item_proxy::resolve_proxy(value)?;
    let value = resolved.as_ref().map_or(value, |v| v.bind(value.py()));
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => {
            for v in arr.iter() {
                if equality::value_eq(v, value)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        ItemRs::ArrayOfTables(aot) => {
            for table in aot.iter() {
                if equality::table_eq(table, value)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        // ScalarProxy overrides __contains__, so this is not reachable from Python.
        _ => Err(unsupported_op(item, "'in'")),
    }
}

/// Python truthiness for a TOML item.
pub(crate) fn item_bool(item: &ItemRs) -> bool {
    match item {
        ItemRs::Table(t) => !t.is_empty(),
        ItemRs::ArrayOfTables(aot) => !aot.is_empty(),
        ItemRs::Value(value) => match value {
            ValueRs::Boolean(b) => *b.value(),
            ValueRs::Integer(i) => *i.value() != 0,
            ValueRs::Float(f) => *f.value() != 0.0,
            ValueRs::String(s) => !s.value().is_empty(),
            ValueRs::Array(a) => !a.is_empty(),
            ValueRs::InlineTable(it) => !it.is_empty(),
            ValueRs::Datetime(_) => true,
        },
        // ItemRs::None is an internal toml_edit placeholder never exposed to Python.
        ItemRs::None => false,
    }
}

/// `repr()` for a TOML item: `Item(type, content)`.
pub(crate) fn item_repr(item: &ItemRs) -> String {
    let type_name = item.type_name();
    let content = item.to_string();
    let trimmed = content.trim();
    format!("Item({type_name}, {trimmed})")
}

/// `str()` for a TOML item: fast path for scalars, falls back to Python for complex types.
pub(crate) fn item_str(item: &ItemRs, py: Python<'_>) -> PyResult<String> {
    // Fast path for scalars: avoid Python object allocation + __str__ call.
    if let ItemRs::Value(v) = item {
        match v {
            ValueRs::String(s) => return Ok(s.value().to_owned()),
            ValueRs::Integer(i) => return Ok(i.value().to_string()),
            ValueRs::Float(_) => {}
            ValueRs::Boolean(b) => return Ok(if *b.value() { "True" } else { "False" }.to_owned()),
            _ => {}
        }
    }
    // Complex types (datetime, table, array, AoT): fall through to Python.
    let obj = item_to_py(item, py)?;
    obj.call_method0(py, "__str__")?.extract::<String>(py)
}

// ---------------------------------------------------------------------------
// Conversion
// ---------------------------------------------------------------------------

/// Convert an `Item` wrapper to a `toml_edit::Value`, or raise `TypeError`.
pub(crate) fn into_value(item: Item) -> PyResult<ValueRs> {
    item.0.into_value().map_err(|item| {
        PyTypeError::new_err(format!(
            "cannot convert {} to a TOML value",
            item.type_name()
        ))
    })
}

/// Convert a toml_edit table's entries to a Python dict.
pub(crate) fn table_to_pydict<'a>(
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
    match value {
        ValueRs::String(s) => Ok(s.value().into_pyobject(py)?.into_any().unbind()),
        ValueRs::Integer(i) => Ok(i.value().into_pyobject(py)?.into_any().unbind()),
        ValueRs::Float(f) => Ok(f.value().into_pyobject(py)?.into_any().unbind()),
        ValueRs::Boolean(b) => Ok(b.value().into_pyobject(py)?.to_owned().into_any().unbind()),
        ValueRs::Array(arr) => {
            let list = PyList::empty(py);
            for v in arr.iter() {
                list.append(value_to_py(v, py)?)?;
            }
            Ok(list.into_any().unbind())
        }
        ValueRs::InlineTable(it) => {
            let dict = PyDict::new(py);
            for (k, v) in it.iter() {
                dict.set_item(k, value_to_py(v, py)?)?;
            }
            Ok(dict.into_any().unbind())
        }
        ValueRs::Datetime(dt) => datetime_to_py(dt.value(), py),
    }
}

/// Convert a toml_edit Datetime to a Python datetime.datetime, date, or time.
pub(crate) fn datetime_to_py(dt: &toml_edit::Datetime, py: Python<'_>) -> PyResult<Py<PyAny>> {
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

/// Result of inspecting a TOML item's iteration shape: either a list of
/// table keys or the length of an array.
pub(crate) enum IterKind<'a> {
    TableKeys(Vec<&'a str>),
    ArrayLen(usize),
}

/// Determine the iteration shape of a TOML item.
pub(crate) fn item_iter_kind<'a>(item: &'a ItemRs) -> PyResult<IterKind<'a>> {
    if let Some(tbl) = item.as_table_like() {
        return Ok(IterKind::TableKeys(tbl.iter().map(|(k, _)| k).collect()));
    }
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => Ok(IterKind::ArrayLen(arr.len())),
        ItemRs::ArrayOfTables(aot) => Ok(IterKind::ArrayLen(aot.len())),
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item is not iterable (use .value to get the Python object)",
            item.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Setitem
// ---------------------------------------------------------------------------

/// Set a string-keyed entry (table / inline table).
/// Returns `Some(key)` if an existing value was replaced, `None` if a new key
/// was added.
pub(crate) fn item_setitem_str(
    item: &mut ItemRs,
    key: String,
    value: Item,
) -> PyResult<Option<Key>> {
    match item {
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => {
            let replaced = item.get(key.as_str()).is_some();
            dict_ops::set_with_decor_preservation(item, &key, value);
            Ok(if replaced { Some(Key::Str(key)) } else { None })
        }
        _ => Err(subscript_type_error(item)),
    }
}

/// Set an integer-keyed entry (array / array of tables).
/// Returns the key of the replaced element.
pub(crate) fn item_setitem_int(item: &mut ItemRs, idx_raw: i64, value: Item) -> PyResult<Key> {
    match item {
        ItemRs::Value(ValueRs::Array(array)) => {
            let idx = list_ops::resolve_index(idx_raw, array.len())?;
            let mut v = into_value(value)?;
            let inline = comments::take_value_inline_comment(&mut v);
            array.replace(idx, v);
            if !inline.is_empty() {
                comments::set_array_inline_comment(array, idx, &inline);
            }
            Ok(Key::Int(idx))
        }
        ItemRs::ArrayOfTables(aot) => {
            let idx = list_ops::resolve_index(idx_raw, aot.len())?;
            let mut table = list_ops::require_table(value)?;
            let saved = list_ops::save_aot_entry_prefix(aot, idx);
            table.decor_mut().set_prefix(&saved);
            aot.replace(idx, table);
            Ok(Key::Int(idx))
        }
        _ => Err(subscript_type_error(item)),
    }
}

// ---------------------------------------------------------------------------
// Delitem
// ---------------------------------------------------------------------------

/// Delete a string-keyed entry (table / inline table).
pub(crate) fn item_delitem_str(item: &mut ItemRs, key: &str) -> PyResult<Affected> {
    if item.as_table_like().is_none() {
        return Err(subscript_type_error(item));
    }
    let (_removed, k) = dict_ops::table_pop(item, key)?;
    Ok(Affected::Child(k))
}

/// Delete an integer-keyed entry (array / array of tables).
pub(crate) fn item_delitem_int(item: &mut ItemRs, idx_raw: i64) -> PyResult<Affected> {
    match item {
        ItemRs::Value(ValueRs::Array(_)) | ItemRs::ArrayOfTables(_) => {
            let target = list_ops::as_array_like_mut(item, "__delitem__")?;
            let idx = list_ops::resolve_index(idx_raw, target.len())?;
            let (_removed, affected) = list_ops::item_remove_at(target, idx)?;
            Ok(affected)
        }
        // Tables: int key can never match → KeyError (dict semantics).
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => {
            Err(PyKeyError::new_err(idx_raw.to_string()))
        }
        _ => Err(subscript_type_error(item)),
    }
}

// ---------------------------------------------------------------------------
// Mutation
// ---------------------------------------------------------------------------

/// Clear all entries from a container item.
pub(crate) fn item_clear(item: &mut ItemRs) -> PyResult<()> {
    if let Some(tbl) = item.as_table_like_mut() {
        tbl.clear();
        return Ok(());
    }
    match list_ops::as_array_like_mut(item, "clear()")? {
        list_ops::ArrayLikeMut::Array(arr) => arr.clear(),
        list_ops::ArrayLikeMut::Aot(aot) => aot.clear(),
    }
    Ok(())
}

/// Normalize formatting of a single item (shallow).
pub(crate) fn item_fmt(item: &mut ItemRs) {
    if let Some(tbl) = item.as_table_like_mut() {
        tbl.fmt();
    } else if let ItemRs::Value(ValueRs::Array(arr)) = item {
        arr.fmt();
    }
}
