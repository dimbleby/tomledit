use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyDict, PyList, PyTime, PyTzInfo};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::item::Item;
use crate::item_proxy::with_proxy_item;
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
    Ok(with_proxy_item(value, |item| match item {
        ItemRs::Value(ValueRs::String(s)) => Some(s.value().to_owned()),
        _ => None,
    })?
    .flatten())
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
