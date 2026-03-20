use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyDict, PyList, PyTime, PyTzInfo};
use toml_edit::DocumentMut as DocumentRs;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::equality;
use crate::item::Item;

// ---------------------------------------------------------------------------
// Inline-table comment-preserving helpers
// ---------------------------------------------------------------------------

/// Remove a key from an inline table, preserving sibling inline comments.
/// Returns the removed value, or `None` if the key was not found.
fn it_remove_preserving(it: &mut toml_edit::InlineTable, key: &str) -> Option<toml_edit::Value> {
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
// Decor preservation
// ---------------------------------------------------------------------------

pub(crate) fn set_with_decor_preservation(item: &mut ItemRs, key: &str, value: Item) {
    // Tables and ArrayOfTables must stay as-is; into_value() would convert
    // a standard Table ([foo]) into an InlineTable (foo = {}).
    // Exception: inside inline tables, nested dicts MUST become inline tables.
    if (value.0.is_table() || value.0.is_array_of_tables()) && !item.is_inline_table() {
        let mut val = value.0;
        // Clear position-specific decor so toml_edit applies its default
        // blank-line-before-header formatting.  Without this, a table
        // cloned from another document would carry the source's decor
        // (e.g. no leading newline when it was the first table there).
        if let Some(t) = val.as_table_mut() {
            t.decor_mut().clear();
            t.set_position(None);
        }
        if let Some(aot) = val.as_array_of_tables_mut() {
            for t in aot.iter_mut() {
                t.decor_mut().clear();
                t.set_position(None);
            }
        }
        item[key] = val;
        return;
    }

    // For new keys in inline tables, preserve sibling inline comments
    // (existing keys don't change key order, so no save/restore needed).
    let saved_ic = item
        .as_inline_table()
        .filter(|it| !it.contains_key(key))
        .map(comments::save_it_inline_comments);

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

    if let Some(mut ic) = saved_ic {
        ic.push(String::new());
        if let Some(it) = item.as_inline_table_mut() {
            comments::restore_it_inline_comments(it, &ic);
        }
    }
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
            ValueRs::Float(f) => return Ok(f.value().to_string()),
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
pub(crate) enum IterKind<'a> {
    TableKeys(Vec<&'a str>),
    ArrayLen(usize),
}

pub(crate) fn item_iter_info<'a>(item: &'a ItemRs) -> PyResult<IterKind<'a>> {
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
pub(crate) fn collect_slice_indices(start: isize, stop: isize, step: isize) -> Vec<usize> {
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
pub(crate) fn require_array_like_len(item: &ItemRs) -> PyResult<usize> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => Ok(arr.len()),
        ItemRs::ArrayOfTables(aot) => Ok(aot.len()),
        _ => Err(unsupported_op(item, "slicing")),
    }
}

/// Resolve an integer index against an array-like item.
fn require_array_index(item: &ItemRs, index: i64) -> PyResult<usize> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => resolve_index(index, arr.len()),
        ItemRs::ArrayOfTables(aot) => resolve_index(index, aot.len()),
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => Err(PyTypeError::new_err(
            "TOML table keys must be strings, not integers",
        )),
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item is not subscriptable (use .value to get the Python object)",
            item.type_name()
        ))),
    }
}

/// Delete elements at the given indices (sorted in reverse internally).
pub(crate) fn item_delitem_slice(item: &mut ItemRs, indices: &[usize]) -> PyResult<()> {
    let mut sorted = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.reverse();

    match item {
        ItemRs::Value(ValueRs::Array(arr)) => {
            let mut ic = comments::save_inline_comments(arr);
            let removing_first = sorted.last() == Some(&0);
            let removing_last = sorted.first() == Some(&(arr.len() - 1));
            let decor = save_removal_decor(arr, removing_first, removing_last);
            for idx in sorted {
                arr.remove(idx);
                ic.remove(idx);
            }
            comments::restore_inline_comments(arr, &ic);
            apply_removal_decor(arr, &decor);
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
pub(crate) fn item_setitem_slice(
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

        let mut ic = comments::save_inline_comments(arr);
        let removes_first = start_idx == 0 && stop_idx > 0;
        let removes_last = stop_idx == arr.len() && stop_idx > start_idx;
        let decor = save_removal_decor(
            arr,
            removes_first && values.is_empty(),
            removes_last && values.is_empty(),
        );

        // Remove old elements from back to front.
        for i in (start_idx..stop_idx).rev() {
            arr.remove(i);
            ic.remove(i);
        }

        // Insert new elements at start position.
        for (offset, value) in values.into_iter().enumerate() {
            let mut v = into_value(value)?;
            let inline = comments::take_inline_comment(&mut v);
            let idx = start_idx + offset;
            if idx >= arr.len() {
                arr.push(v);
            } else {
                arr.insert(idx, v);
            }
            ic.insert(idx, inline);
        }
        comments::restore_inline_comments(arr, &ic);
        apply_removal_decor(arr, &decor);
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
            let mut v = into_value(value)?;
            let inline = comments::take_inline_comment(&mut v);
            arr.replace(idx, v);
            if !inline.is_empty() {
                comments::set_array_item_comment(arr, idx, &inline);
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Array boundary-element decoration repair
// ---------------------------------------------------------------------------

/// Decoration state captured before an array removal so that the opening and
/// closing brackets stay in their original positions.
struct RemovalDecor {
    /// Prefix of the old first element (between `[` and the value).
    first_prefix: Option<String>,
    /// Suffix of the old last element (between the value and `]`).
    last_suffix: Option<String>,
}

/// Snapshot the decorations that would be lost when the first and/or last
/// element of an array is removed.
///
/// `removing_first` / `removing_last` indicate whether the removal will
/// affect element 0 or element `len − 1`.  Returns `None` fields when the
/// corresponding boundary is unaffected or the array is too small to need
/// repair (single-element arrays becoming empty).
fn save_removal_decor(
    arr: &toml_edit::Array,
    removing_first: bool,
    removing_last: bool,
) -> RemovalDecor {
    let at_least_two = arr.len() >= 2;
    RemovalDecor {
        first_prefix: (removing_first && at_least_two).then(|| {
            arr.get(0)
                .and_then(|v| v.decor().prefix().and_then(|r| r.as_str()))
                .unwrap_or_default()
                .to_owned()
        }),
        last_suffix: (removing_last && at_least_two).then(|| {
            let last = arr.len() - 1;
            arr.get(last)
                .and_then(|v| v.decor().suffix().and_then(|r| r.as_str()))
                .unwrap_or_default()
                .to_owned()
        }),
    }
}

/// Apply saved decoration fixes after a removal + `restore_inline_comments`.
fn apply_removal_decor(arr: &mut toml_edit::Array, decor: &RemovalDecor) {
    // --- First-element prefix ---
    // The new first element inherits a prefix meant to follow a comma (e.g.
    // `" "` in `[1, 2]` or `" # note\n    "` in multiline arrays).  Replace
    // it with the old first element's prefix so `[1, 2, 3]` becomes `[2, 3]`
    // instead of `[ 2, 3]`.
    if let Some(ref old_first_prefix) = decor.first_prefix
        && let Some(new_first) = arr.get_mut(0)
    {
        let cur = new_first
            .decor()
            .prefix()
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_owned();
        let fixed = if let Some((_inline, rest)) = cur.split_once('\n') {
            // Multiline: drop the removed element's inline-comment line,
            // keep block comments + indentation that belong to this element.
            format!("\n{rest}")
        } else {
            // Single-line: use the original first element's prefix.
            old_first_prefix.clone()
        };
        new_first.decor_mut().set_prefix(&fixed);
    }

    // --- Last-element suffix ---
    // The old last element's suffix (whitespace between the value and `]`) is
    // discarded by toml_edit when that element is removed.  Transfer it to the
    // array's trailing string so the closing bracket stays in place, e.g.
    // `[ 1, 2, 3 ]` becomes `[ 1, 2 ]` instead of `[ 1, 2]`.
    if let Some(ref old_last_suffix) = decor.last_suffix
        && !old_last_suffix.is_empty()
        && !arr.is_empty()
    {
        let trailing = arr.trailing().as_str().unwrap_or_default().to_owned();
        arr.set_trailing(format!("{trailing}{old_last_suffix}"));
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
        if key.extract::<i64>().is_ok() {
            PyTypeError::new_err("TOML table keys must be strings, not integers")
        } else {
            bad_key_type(key)
        }
    })
}

fn require_int_key(key: &Bound<'_, PyAny>) -> PyResult<i64> {
    key.extract().map_err(|_| {
        if key.extract::<String>().is_ok() {
            PyTypeError::new_err("TOML array indices must be integers, not strings")
        } else {
            bad_key_type(key)
        }
    })
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

fn require_table(item: Item) -> PyResult<toml_edit::Table> {
    match item.0 {
        ItemRs::Table(t) => Ok(t),
        ItemRs::Value(ValueRs::InlineTable(it)) => Ok(it.into_table()),
        other => Err(PyTypeError::new_err(format!(
            "cannot append {} to array of tables (expected a table/dict)",
            other.type_name()
        ))),
    }
}

pub(crate) fn item_keys(item: &ItemRs) -> PyResult<Vec<String>> {
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

pub(crate) fn item_has_key(item: &ItemRs, key: &str) -> PyResult<bool> {
    match item {
        ItemRs::Table(table) => Ok(table.contains_key(key)),
        ItemRs::Value(ValueRs::InlineTable(it)) => Ok(it.contains_key(key)),
        ItemRs::Value(ValueRs::Array(_)) | ItemRs::ArrayOfTables(_) => Err(PyTypeError::new_err(
            "TOML array indices must be integers, not strings",
        )),
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item is not subscriptable (use .value to get the Python object)",
            item.type_name()
        ))),
    }
}

// ---------------------------------------------------------------------------
// Getitem
// ---------------------------------------------------------------------------

pub(crate) fn item_getitem(item: &ItemRs, key: &Bound<'_, PyAny>) -> PyResult<Key> {
    if let Ok(k) = key.extract::<String>() {
        if !item_has_key(item, &k)? {
            return Err(PyKeyError::new_err(k));
        }
        Ok(Key::Str(k))
    } else if let Ok(k) = key.extract::<i64>() {
        Ok(Key::Int(require_array_index(item, k)?))
    } else {
        Err(bad_key_type(key))
    }
}

// ---------------------------------------------------------------------------
// Setitem
// ---------------------------------------------------------------------------

/// Returns `true` if an existing value was replaced, `false` if a new key was added.
pub(crate) fn item_setitem(
    item: &mut ItemRs,
    key: &Bound<'_, PyAny>,
    value: Item,
) -> PyResult<Option<Key>> {
    match item {
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => {
            let k = require_str_key(key)?;
            let replaced = item.get(k.as_str()).is_some();
            set_with_decor_preservation(item, &k, value);
            Ok(if replaced { Some(Key::Str(k)) } else { None })
        }
        ItemRs::Value(ValueRs::Array(array)) => {
            let idx = resolve_index(require_int_key(key)?, array.len())?;
            let mut v = into_value(value)?;
            let inline = comments::take_inline_comment(&mut v);
            array.replace(idx, v);
            if !inline.is_empty() {
                comments::set_array_item_comment(array, idx, &inline);
            }
            Ok(Some(Key::Int(idx)))
        }
        ItemRs::ArrayOfTables(aot) => {
            let idx = resolve_index(require_int_key(key)?, aot.len())?;
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

pub(crate) fn item_delitem(item: &mut ItemRs, key: &Bound<'_, PyAny>) -> PyResult<Key> {
    match item {
        ItemRs::Table(table) => {
            let k = require_str_key(key)?;
            if table.remove(&k).is_none() {
                return Err(PyKeyError::new_err(k));
            }
            Ok(Key::Str(k))
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let k = require_str_key(key)?;
            if it_remove_preserving(it, &k).is_none() {
                return Err(PyKeyError::new_err(k));
            }
            Ok(Key::Str(k))
        }
        ItemRs::Value(ValueRs::Array(array)) => {
            let idx = resolve_index(require_int_key(key)?, array.len())?;
            let mut ic = comments::save_inline_comments(array);
            let decor = save_removal_decor(array, idx == 0, idx == array.len() - 1);
            array.remove(idx);
            ic.remove(idx);
            comments::restore_inline_comments(array, &ic);
            apply_removal_decor(array, &decor);
            Ok(Key::Int(idx))
        }
        ItemRs::ArrayOfTables(aot) => {
            let idx = resolve_index(require_int_key(key)?, aot.len())?;
            aot.remove(idx);
            Ok(Key::Int(idx))
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

pub(crate) fn item_pop(item: &mut ItemRs, key: Option<&Bound<'_, PyAny>>) -> PyResult<Item> {
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
                it_remove_preserving(it, key)
                    .map(|v| Item(ItemRs::Value(v)))
                    .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
            }
            ItemRs::Value(ValueRs::Array(arr)) => {
                let idx = resolve_index(key_obj.extract::<i64>()?, arr.len())?;
                let mut ic = comments::save_inline_comments(arr);
                let decor = save_removal_decor(arr, idx == 0, idx == arr.len() - 1);
                let removed = arr.remove(idx);
                ic.remove(idx);
                comments::restore_inline_comments(arr, &ic);
                apply_removal_decor(arr, &decor);
                Ok(Item(ItemRs::Value(removed)))
            }
            ItemRs::ArrayOfTables(aot) => {
                let idx = resolve_index(key_obj.extract::<i64>()?, aot.len())?;
                Ok(Item(ItemRs::Table(aot.remove(idx))))
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
                let decor = save_removal_decor(arr, false, true);
                let removed = arr.remove(last);
                ic.remove(last);
                comments::restore_inline_comments(arr, &ic);
                apply_removal_decor(arr, &decor);
                Ok(Item(ItemRs::Value(removed)))
            }
            ItemRs::ArrayOfTables(aot) => {
                if aot.is_empty() {
                    return Err(PyIndexError::new_err("pop from empty array"));
                }
                let last = aot.len() - 1;
                Ok(Item(ItemRs::Table(aot.remove(last))))
            }
            _ => Err(PyTypeError::new_err(
                "pop() with no argument is only supported on arrays",
            )),
        },
    }
}

/// Extract key-value pairs from a Python object for update().
///
/// Follows the same protocol as dict.update:
/// - If the object is a dict, iterate its entries directly.
/// - If the object has a `.keys()` method, iterate keys and index for values.
/// - Otherwise, iterate as (key, value) pairs.
///
/// All paths pre-collect into a Vec because values may be ItemProxy objects
/// referencing the same document, and extracting them borrows the document.
pub(crate) fn extract_update_pairs(other: &Bound<'_, PyAny>) -> PyResult<Vec<(String, Item)>> {
    if let Ok(dict) = other.cast::<PyDict>() {
        let mut pairs = Vec::with_capacity(dict.len());
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            let val: Item = v.extract()?;
            pairs.push((key, val));
        }
        return Ok(pairs);
    }

    // Mapping with .keys()
    if let Ok(keys_method) = other.getattr("keys") {
        let keys = keys_method.call0()?;
        let mut pairs = Vec::new();
        for key_obj in keys.try_iter()? {
            let key_obj = key_obj?;
            let key: String = key_obj.extract()?;
            let val: Item = other.get_item(&key_obj)?.extract()?;
            pairs.push((key, val));
        }
        return Ok(pairs);
    }

    // Iterable of (key, value) pairs
    let mut pairs = Vec::new();
    for item in other.try_iter()? {
        let item = item?;
        let (key, val): (String, Item) = item.extract()?;
        pairs.push((key, val));
    }
    Ok(pairs)
}

/// Apply pre-extracted update pairs to an item.
///
/// Returns `true` if any key replaced an entry that existed before the update.
pub(crate) fn apply_update_pairs(
    item: &mut ItemRs,
    pairs: Vec<(String, Item)>,
) -> PyResult<Vec<String>> {
    if !(item.is_table() || item.is_inline_table()) {
        return Err(unsupported_op(item, "update()"));
    }
    let mut replaced_keys = Vec::new();
    for (key, val) in pairs {
        let exists = item.as_table().is_some_and(|t| t.contains_key(&key))
            || item.as_inline_table().is_some_and(|t| t.contains_key(&key));
        if exists {
            replaced_keys.push(key.clone());
        }
        set_with_decor_preservation(item, &key, val);
    }
    Ok(replaced_keys)
}

// ---------------------------------------------------------------------------
// Mutation: list-like
// ---------------------------------------------------------------------------

/// Detect whether an array uses multiline formatting and return the element
/// decor prefix if so (e.g. `"\n    "`).  Returns `None` for single-line arrays.
fn multiline_prefix(arr: &toml_edit::Array) -> Option<String> {
    let first = arr.get(0)?;
    let raw = first.decor().prefix()?.as_str()?;
    if raw.contains('\n') {
        Some(raw.to_owned())
    } else {
        None
    }
}

/// Apply multiline decor to a newly created value, matching the array's style.
fn apply_multiline_decor(arr: &toml_edit::Array, v: &mut ValueRs) {
    if let Some(prefix) = multiline_prefix(arr) {
        let decor = v.decor_mut();
        decor.set_prefix(prefix);
        decor.set_suffix("");
    }
}

pub(crate) fn item_append(item: &mut ItemRs, value: Item) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        let mut ic = comments::save_inline_comments(arr);
        let mut v = into_value(value)?;
        let inline = comments::take_inline_comment(&mut v);
        apply_multiline_decor(arr, &mut v);
        arr.push(v);
        ic.push(inline);
        comments::restore_inline_comments(arr, &ic);
        Ok(())
    } else if let ItemRs::ArrayOfTables(aot) = item {
        let table = require_table(value)?;
        aot.push(table);
        Ok(())
    } else {
        Err(unsupported_op(item, "append()"))
    }
}

pub(crate) fn item_insert(item: &mut ItemRs, index: i64, value: Item) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        let resolved = clamp_index(index, arr.len());
        let mut ic = comments::save_inline_comments(arr);
        let mut v = into_value(value)?;
        let inline = comments::take_inline_comment(&mut v);
        apply_multiline_decor(arr, &mut v);
        arr.insert(resolved, v);
        ic.insert(resolved, inline);
        comments::restore_inline_comments(arr, &ic);
        Ok(())
    } else if let ItemRs::ArrayOfTables(aot) = item {
        let table = require_table(value)?;
        let resolved = clamp_index(index, aot.len());
        // AoT has no insert API; rebuild by removing the tail, pushing, and restoring.
        let mut tail: Vec<toml_edit::Table> =
            (resolved..aot.len()).rev().map(|i| aot.remove(i)).collect();
        tail.reverse();
        aot.push(table);
        for t in tail {
            aot.push(t);
        }
        Ok(())
    } else {
        Err(unsupported_op(item, "insert()"))
    }
}

pub(crate) fn item_remove(item: &mut ItemRs, value: &Bound<'_, PyAny>) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        let mut ic = comments::save_inline_comments(arr);
        // We don't know which element will match, so snapshot both boundaries.
        let mut decor = save_removal_decor(arr, true, true);
        for i in 0..arr.len() {
            if let Some(v) = arr.get(i)
                && equality::value_eq(v, value)?
            {
                let last = arr.len() - 1;
                if i != 0 {
                    decor.first_prefix = None;
                }
                if i != last {
                    decor.last_suffix = None;
                }
                arr.remove(i);
                ic.remove(i);
                comments::restore_inline_comments(arr, &ic);
                apply_removal_decor(arr, &decor);
                return Ok(());
            }
        }
        Err(PyValueError::new_err("value not in array"))
    } else if let ItemRs::ArrayOfTables(aot) = item {
        if let Ok(other_dict) = value.cast::<PyDict>() {
            for i in 0..aot.len() {
                if let Some(table) = aot.get(i)
                    && equality::table_entries_eq(table.iter(), table.len(), other_dict)?
                {
                    aot.remove(i);
                    return Ok(());
                }
            }
        }
        Err(PyValueError::new_err("value not in array"))
    } else {
        Err(unsupported_op(item, "remove()"))
    }
}

pub(crate) fn item_extend(item: &mut ItemRs, items: Vec<Item>) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        let mut ic = comments::save_inline_comments(arr);
        for new_item in items {
            let mut v = into_value(new_item)?;
            let inline = comments::take_inline_comment(&mut v);
            apply_multiline_decor(arr, &mut v);
            arr.push(v);
            ic.push(inline);
        }
        comments::restore_inline_comments(arr, &ic);
        Ok(())
    } else if let ItemRs::ArrayOfTables(aot) = item {
        for new_item in items {
            let table = require_table(new_item)?;
            aot.push(table);
        }
        Ok(())
    } else {
        Err(unsupported_op(item, "extend()"))
    }
}

pub(crate) fn item_count(item: &ItemRs, value: &Bound<'_, PyAny>) -> PyResult<usize> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => {
            let mut count = 0;
            for v in arr.iter() {
                if equality::value_eq(v, value)? {
                    count += 1;
                }
            }
            Ok(count)
        }
        ItemRs::ArrayOfTables(aot) => {
            if let Ok(other_dict) = value.cast::<PyDict>() {
                let mut count = 0;
                for table in aot.iter() {
                    if equality::table_entries_eq(table.iter(), table.len(), other_dict)? {
                        count += 1;
                    }
                }
                Ok(count)
            } else {
                Ok(0)
            }
        }
        _ => Err(unsupported_op(item, "count()")),
    }
}

pub(crate) fn item_index(
    item: &ItemRs,
    value: &Bound<'_, PyAny>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<usize> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => {
            let len = arr.len();
            let start = clamp_index(start.unwrap_or(0), len);
            let stop = clamp_index(stop.unwrap_or(len as i64), len);
            for i in start..stop {
                if let Some(v) = arr.get(i)
                    && equality::value_eq(v, value)?
                {
                    return Ok(i);
                }
            }
            Err(PyValueError::new_err("value not in array"))
        }
        ItemRs::ArrayOfTables(aot) => {
            let len = aot.len();
            let start = clamp_index(start.unwrap_or(0), len);
            let stop = clamp_index(stop.unwrap_or(len as i64), len);
            if let Ok(other_dict) = value.cast::<PyDict>() {
                for i in start..stop {
                    if let Some(table) = aot.get(i)
                        && equality::table_entries_eq(table.iter(), table.len(), other_dict)?
                    {
                        return Ok(i);
                    }
                }
            }
            Err(PyValueError::new_err("value not in array"))
        }
        _ => Err(unsupported_op(item, "index()")),
    }
}

/// Clamp a signed index to `0..len` (negative counts from end, out-of-range clamps).
fn clamp_index(index: i64, len: usize) -> usize {
    let resolved = if index < 0 {
        (len as i64 + index).max(0)
    } else {
        index.min(len as i64)
    };
    resolved as usize
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

/// Format an array as multiline, with each element on its own line.
/// No-op on empty arrays.
pub(crate) fn item_set_multiline(item: &mut ItemRs, indent: usize) -> PyResult<()> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => {
            if !arr.is_empty() {
                let prefix = format!("\n{}", " ".repeat(indent));
                for val in arr.iter_mut() {
                    let decor = val.decor_mut();
                    decor.set_prefix(&prefix);
                    decor.set_suffix("");
                }
                arr.set_trailing_comma(true);
                arr.set_trailing("\n");
            }
            Ok(())
        }
        _ => Err(unsupported_op(item, "set_multiline()")),
    }
}

// ---------------------------------------------------------------------------
// Key type (shared with item_proxy)
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
