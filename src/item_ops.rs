use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyDict, PyList, PyTime, PyTzInfo};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::dict_ops;
use crate::equality;
use crate::item::Item;
use crate::list_ops;

// ---------------------------------------------------------------------------
// Inline-table comment-preserving helpers
// ---------------------------------------------------------------------------

/// Remove a key from an inline table, preserving sibling inline comments.
/// Returns the removed value, or `None` if the key was not found.
fn it_remove(it: &mut toml_edit::InlineTable, key: &str) -> Option<toml_edit::Value> {
    let mut ic = comments::save_it_inline_comments(it);
    let pos = comments::it_key_position(it, key);
    let removed = it.remove(key)?;
    if let Some(pos) = pos {
        ic.remove(pos);
    }
    comments::restore_it_inline_comments(it, &ic);
    Some(removed)
}

// ---------------------------------------------------------------------------
// Read operations
// ---------------------------------------------------------------------------

pub(crate) fn item_len(item: &ItemRs) -> Option<usize> {
    match item {
        ItemRs::Table(t) => Some(t.len()),
        ItemRs::Value(ValueRs::Array(a)) => Some(a.len()),
        ItemRs::Value(ValueRs::InlineTable(it)) => Some(it.len()),
        ItemRs::ArrayOfTables(aot) => Some(aot.len()),
        _ => None,
    }
}

pub(crate) fn item_contains(item: &ItemRs, value: &Bound<'_, PyAny>) -> PyResult<bool> {
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
        ItemRs::None => false,
    }
}

pub(crate) fn item_repr(item: &ItemRs) -> String {
    let type_name = item.type_name();
    let content = item.to_string();
    let trimmed = content.trim();
    format!("Item({type_name}, {trimmed})")
}

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

pub(crate) fn item_iter_kind<'a>(item: &'a ItemRs) -> PyResult<IterKind<'a>> {
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

fn require_str_key(key: &Bound<'_, PyAny>) -> PyResult<String> {
    key.extract().map_err(|_| {
        // Use a C-level type check instead of extract (which invokes
        // __index__) to avoid re-borrowing the document through dunder
        // methods when the key is a proxy.
        if key.is_instance_of::<pyo3::types::PyInt>() {
            PyTypeError::new_err("TOML table keys must be strings, not integers")
        } else {
            bad_key_type(key)
        }
    })
}

fn require_int_key(key: &Bound<'_, PyAny>) -> PyResult<i64> {
    key.extract().map_err(|_| {
        if key.is_instance_of::<pyo3::types::PyString>() {
            PyTypeError::new_err("TOML array indices must be integers, not strings")
        } else {
            bad_key_type(key)
        }
    })
}

pub(crate) fn unsupported_op(item: &ItemRs, op: &str) -> PyErr {
    PyTypeError::new_err(format!(
        "TOML {} item does not support {op}",
        item.type_name()
    ))
}

pub(crate) fn into_value(item: Item) -> PyResult<ValueRs> {
    item.0.into_value().map_err(|item| {
        PyTypeError::new_err(format!(
            "cannot convert {} to a TOML value",
            item.type_name()
        ))
    })
}

// ---------------------------------------------------------------------------
// Getitem
// ---------------------------------------------------------------------------

pub(crate) fn item_getitem(item: &ItemRs, key: &Bound<'_, PyAny>) -> PyResult<Key> {
    if let Ok(k) = key.extract::<String>() {
        if !dict_ops::item_has_key(item, &k)? {
            return Err(PyKeyError::new_err(k));
        }
        Ok(Key::Str(k))
    } else if let Ok(k) = key.extract::<i64>() {
        Ok(Key::Int(list_ops::require_array_index(item, k)?))
    } else {
        Err(bad_key_type(key))
    }
}

// ---------------------------------------------------------------------------
// Setitem
// ---------------------------------------------------------------------------

/// Returns `Some(key)` if an existing value was replaced, `None` if a new key was added.
pub(crate) fn item_setitem(
    item: &mut ItemRs,
    key: &Bound<'_, PyAny>,
    value: Item,
) -> PyResult<Option<Key>> {
    match item {
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => {
            let k = require_str_key(key)?;
            let replaced = item.get(k.as_str()).is_some();
            dict_ops::set_with_decor_preservation(item, &k, value);
            Ok(if replaced { Some(Key::Str(k)) } else { None })
        }
        ItemRs::Value(ValueRs::Array(array)) => {
            let idx = list_ops::resolve_index(require_int_key(key)?, array.len())?;
            let mut v = into_value(value)?;
            let inline = comments::take_inline_comment(&mut v);
            array.replace(idx, v);
            if !inline.is_empty() {
                comments::set_array_item_comment(array, idx, &inline);
            }
            Ok(Some(Key::Int(idx)))
        }
        ItemRs::ArrayOfTables(aot) => {
            let idx = list_ops::resolve_index(require_int_key(key)?, aot.len())?;
            if !value.0.is_table() && !value.0.is_inline_table() {
                return Err(PyTypeError::new_err(format!(
                    "cannot assign {} to array of tables (expected a table/dict)",
                    value.0.type_name()
                )));
            }
            item[idx] = value.0;
            Ok(Some(Key::Int(idx)))
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

/// Delete an element.  Returns `Some(key)` when only that key was removed
/// (no index shifting — safe for targeted invalidation), or `None` when
/// indices shifted and the whole container must be invalidated.
pub(crate) fn item_delitem(item: &mut ItemRs, key: &Bound<'_, PyAny>) -> PyResult<Option<Key>> {
    match item {
        ItemRs::Table(table) => {
            let k = require_str_key(key)?;
            if table.remove(&k).is_none() {
                return Err(PyKeyError::new_err(k));
            }
            Ok(Some(Key::Str(k)))
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let k = require_str_key(key)?;
            if it_remove(it, &k).is_none() {
                return Err(PyKeyError::new_err(k));
            }
            Ok(Some(Key::Str(k)))
        }
        ItemRs::Value(ValueRs::Array(array)) => {
            let idx = list_ops::resolve_index(require_int_key(key)?, array.len())?;
            let is_last = idx == array.len() - 1;
            let mut ic = comments::save_inline_comments(array);
            let decor = list_ops::save_removal_decor(array, idx == 0, is_last);
            array.remove(idx);
            ic.remove(idx);
            comments::restore_inline_comments(array, &ic);
            list_ops::apply_removal_decor(array, &decor);
            Ok(is_last.then_some(Key::Int(idx)))
        }
        ItemRs::ArrayOfTables(aot) => {
            let idx = list_ops::resolve_index(require_int_key(key)?, aot.len())?;
            let is_last = idx == aot.len() - 1;
            aot.remove(idx);
            Ok(is_last.then_some(Key::Int(idx)))
        }
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item is not subscriptable",
            item.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Mutation: dict-like
// ---------------------------------------------------------------------------

/// Pop an element.  Returns `(removed_item, affected_key)` where
/// `affected_key` is `Some(key)` when only that key was removed (no index
/// shifting — safe for targeted invalidation), or `None` when indices
/// shifted and the whole container must be invalidated.
pub(crate) fn item_pop(
    item: &mut ItemRs,
    key: Option<&Bound<'_, PyAny>>,
) -> PyResult<(Item, Option<Key>)> {
    match key {
        Some(key_obj) => match item {
            ItemRs::Table(table) => {
                let key: &str = key_obj.extract()?;
                match table.remove(key) {
                    Some(v) => Ok((Item(v), Some(Key::Str(key.into())))),
                    None => Err(PyKeyError::new_err(key.to_owned())),
                }
            }
            ItemRs::Value(ValueRs::InlineTable(it)) => {
                let key: &str = key_obj.extract()?;
                match it_remove(it, key) {
                    Some(v) => Ok((Item(ItemRs::Value(v)), Some(Key::Str(key.into())))),
                    None => Err(PyKeyError::new_err(key.to_owned())),
                }
            }
            ItemRs::Value(ValueRs::Array(arr)) => {
                let idx = list_ops::resolve_index(key_obj.extract::<i64>()?, arr.len())?;
                let is_last = idx == arr.len() - 1;
                let mut ic = comments::save_inline_comments(arr);
                let decor = list_ops::save_removal_decor(arr, idx == 0, is_last);
                let removed = arr.remove(idx);
                ic.remove(idx);
                comments::restore_inline_comments(arr, &ic);
                list_ops::apply_removal_decor(arr, &decor);
                let key = is_last.then_some(Key::Int(idx));
                Ok((Item(ItemRs::Value(removed)), key))
            }
            ItemRs::ArrayOfTables(aot) => {
                let idx = list_ops::resolve_index(key_obj.extract::<i64>()?, aot.len())?;
                let is_last = idx == aot.len() - 1;
                let key = is_last.then_some(Key::Int(idx));
                Ok((Item(ItemRs::Table(aot.remove(idx))), key))
            }
            _ => Err(unsupported_op(item, "pop()")),
        },
        None => match item {
            ItemRs::Value(ValueRs::Array(arr)) => {
                if arr.is_empty() {
                    return Err(PyIndexError::new_err("pop from empty array"));
                }
                let last = arr.len() - 1;
                let mut ic = comments::save_inline_comments(arr);
                let decor = list_ops::save_removal_decor(arr, false, true);
                let removed = arr.remove(last);
                ic.remove(last);
                comments::restore_inline_comments(arr, &ic);
                list_ops::apply_removal_decor(arr, &decor);
                Ok((Item(ItemRs::Value(removed)), Some(Key::Int(last))))
            }
            ItemRs::ArrayOfTables(aot) => {
                if aot.is_empty() {
                    return Err(PyIndexError::new_err("pop from empty array"));
                }
                let last = aot.len() - 1;
                Ok((Item(ItemRs::Table(aot.remove(last))), Some(Key::Int(last))))
            }
            _ => Err(PyTypeError::new_err(
                "pop() with no argument is only supported on arrays",
            )),
        },
    }
}

pub(crate) fn item_clear(item: &mut ItemRs) -> PyResult<()> {
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
pub(crate) fn item_fmt(item: &mut ItemRs) {
    match item {
        ItemRs::Table(table) => table.fmt(),
        ItemRs::Value(ValueRs::InlineTable(it)) => it.fmt(),
        ItemRs::Value(ValueRs::Array(arr)) => arr.fmt(),
        _ => {} // ArrayOfTables, scalars: no-op
    }
}

// ---------------------------------------------------------------------------
// Key type
// ---------------------------------------------------------------------------

#[derive(Clone, Hash, PartialEq, Eq)]
pub(crate) enum Key {
    Str(String),
    Int(usize),
}

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
