//! Shared operations on `toml_edit::Item` used by both `Item` and `ItemProxy`.

use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDate, PyDateTime, PyDelta, PyDict, PyList, PyTime, PyTzInfo};
use toml_edit::Item as ItemRs;

use crate::item::Item;
use crate::value::Datetime;

// ---------------------------------------------------------------------------
// Equality
// ---------------------------------------------------------------------------

/// Semantically compare two toml_edit Datetimes, treating Offset::Z and
/// Offset::Custom { minutes: 0 } as equivalent, and normalizing optional
/// second/nanosecond fields (None == Some(0)).
fn datetime_eq(a: &toml_edit::Datetime, b: &toml_edit::Datetime) -> bool {
    use toml_edit::Offset;

    fn normalize_offset(o: &Option<Offset>) -> Option<i16> {
        o.map(|off| match off {
            Offset::Z => 0,
            Offset::Custom { minutes } => minutes,
        })
    }

    fn time_eq(a: &Option<toml_edit::Time>, b: &Option<toml_edit::Time>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => {
                a.hour == b.hour
                    && a.minute == b.minute
                    && a.second.unwrap_or(0) == b.second.unwrap_or(0)
                    && a.nanosecond.unwrap_or(0) == b.nanosecond.unwrap_or(0)
            }
            (None, None) => true,
            _ => false,
        }
    }

    a.date == b.date
        && time_eq(&a.time, &b.time)
        && normalize_offset(&a.offset) == normalize_offset(&b.offset)
}

/// Compare a toml_edit Value to a Python object for equality.
pub(crate) fn value_eq(value: &toml_edit::Value, other: &Bound<'_, PyAny>) -> PyResult<bool> {
    // Check bool before int (Python bool is subclass of int)
    if let Some(b) = value.as_bool()
        && let Ok(other_b) = other.extract::<bool>()
    {
        return Ok(b == other_b);
    }
    if let Some(i) = value.as_integer()
        && let Ok(other_i) = other.extract::<i64>()
    {
        return Ok(i == other_i);
    }
    if let Some(f) = value.as_float()
        && let Ok(other_f) = other.extract::<f64>()
    {
        return Ok(f == other_f);
    }
    if let Some(s) = value.as_str()
        && let Ok(other_s) = other.extract::<String>()
    {
        return Ok(s == other_s);
    }
    if let Some(dt) = value.as_datetime()
        && let Ok(py_dt) = other.cast::<PyDateTime>()
    {
        let other_dt: Datetime = py_dt.extract()?;
        return Ok(datetime_eq(dt, &other_dt.0));
    }
    // Array == list
    if let Some(arr) = value.as_array()
        && let Ok(other_list) = other.cast::<PyList>()
    {
        if arr.len() != other_list.len() {
            return Ok(false);
        }
        for (i, v) in arr.iter().enumerate() {
            let other_elem = other_list.get_item(i)?;
            if !value_eq(v, &other_elem)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    // InlineTable == dict
    if let Some(it) = value.as_inline_table()
        && let Ok(other_dict) = other.cast::<PyDict>()
    {
        if it.len() != other_dict.len() {
            return Ok(false);
        }
        for (k, v) in it.iter() {
            let Some(other_v) = other_dict.get_item(k)? else {
                return Ok(false);
            };
            if !value_eq(v, &other_v)? {
                return Ok(false);
            }
        }
        return Ok(true);
    }
    Ok(false)
}

/// Compare a toml_edit Table's entries to a Python dict for equality.
fn table_entries_eq<'a>(
    iter: impl Iterator<Item = (&'a str, &'a ItemRs)>,
    len: usize,
    other_dict: &Bound<'_, PyDict>,
) -> PyResult<bool> {
    if len != other_dict.len() {
        return Ok(false);
    }
    for (k, v) in iter {
        let Some(other_v) = other_dict.get_item(k)? else {
            return Ok(false);
        };
        if !item_eq(v, &other_v)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Compare a toml_edit Item to a Python object for equality.
pub(crate) fn item_eq(item: &ItemRs, other: &Bound<'_, PyAny>) -> PyResult<bool> {
    match item {
        ItemRs::Value(value) => value_eq(value, other),
        ItemRs::Table(table) => {
            let Ok(other_dict) = other.cast::<PyDict>() else {
                return Ok(false);
            };
            table_entries_eq(table.iter(), table.len(), other_dict)
        }
        ItemRs::ArrayOfTables(aot) => {
            let Ok(other_list) = other.cast::<PyList>() else {
                return Ok(false);
            };
            if aot.len() != other_list.len() {
                return Ok(false);
            }
            for (i, table) in aot.iter().enumerate() {
                let other_elem = other_list.get_item(i)?;
                let Ok(other_dict) = other_elem.cast::<PyDict>() else {
                    return Ok(false);
                };
                if !table_entries_eq(table.iter(), table.len(), other_dict)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

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

pub(crate) fn item_len(item: &ItemRs) -> Option<usize> {
    if let Some(t) = item.as_table() {
        Some(t.len())
    } else if let Some(a) = item.as_array() {
        Some(a.len())
    } else if let Some(it) = item.as_inline_table() {
        Some(it.len())
    } else {
        item.as_array_of_tables().map(|aot| aot.len())
    }
}

pub(crate) fn item_contains(item: &ItemRs, value: &Bound<'_, PyAny>) -> PyResult<bool> {
    if let Some(table) = item.as_table() {
        let key: String = value.extract()?;
        Ok(table.contains_key(&key))
    } else if let Some(it) = item.as_inline_table() {
        let key: String = value.extract()?;
        Ok(it.contains_key(&key))
    } else if let Some(arr) = item.as_array() {
        for v in arr.iter() {
            if value_eq(v, value)? {
                return Ok(true);
            }
        }
        Ok(false)
    } else if let Some(aot) = item.as_array_of_tables() {
        if let Ok(idx) = value.extract::<usize>() {
            Ok(idx < aot.len())
        } else {
            Ok(false)
        }
    } else {
        Err(PyTypeError::new_err(
            "argument of type 'scalar' is not iterable",
        ))
    }
}

pub(crate) fn item_bool(item: &ItemRs) -> bool {
    match item_len(item) {
        Some(len) => len > 0,
        None => true, // scalars are truthy
    }
}

pub(crate) fn item_repr(item: &ItemRs, type_label: &str) -> String {
    let type_name = item.type_name();
    let content = item.to_string();
    let trimmed = content.trim();
    format!("{type_label}({type_name}, {trimmed})")
}

pub(crate) fn item_str(item: &ItemRs, py: Python<'_>) -> PyResult<String> {
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
pub(crate) enum IterKind {
    TableKeys(Vec<String>),
    ArrayLen(usize),
}

pub(crate) fn item_iter_info(item: &ItemRs) -> PyResult<IterKind> {
    if let Some(table) = item.as_table() {
        Ok(IterKind::TableKeys(
            table.iter().map(|(k, _)| k.to_owned()).collect(),
        ))
    } else if let Some(it) = item.as_inline_table() {
        Ok(IterKind::TableKeys(
            it.iter().map(|(k, _)| k.to_owned()).collect(),
        ))
    } else if let Some(arr) = item.as_array() {
        Ok(IterKind::ArrayLen(arr.len()))
    } else if let Some(aot) = item.as_array_of_tables() {
        Ok(IterKind::ArrayLen(aot.len()))
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' is not iterable",
            item.type_name()
        )))
    }
}

/// Resolve a Python index (possibly negative) against a known length.
/// Returns the resolved non-negative index, or an IndexError.
pub(crate) fn resolve_index(index: i64, len: usize) -> PyResult<usize> {
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

/// Collect concrete indices from resolved slice parameters (as returned by PySlice::indices()).
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
pub(crate) fn item_delitem_slice(item: &mut ItemRs, indices: &[usize]) -> PyResult<()> {
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

/// Build a proper TypeError for bad key types in __getitem__.
pub(crate) fn bad_key_type(key: &Bound<'_, PyAny>) -> PyErr {
    let type_name = key
        .get_type()
        .name()
        .map(|n| n.to_string())
        .unwrap_or_else(|_| "?".to_owned());
    PyTypeError::new_err(format!(
        "indices must be integers or strings, not {type_name}"
    ))
}

pub(crate) fn item_keys(item: &ItemRs) -> PyResult<Vec<String>> {
    if let Some(table) = item.as_table() {
        Ok(table.iter().map(|(k, _)| k.to_owned()).collect())
    } else if let Some(it) = item.as_inline_table() {
        Ok(it.iter().map(|(k, _)| k.to_owned()).collect())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' has no keys()",
            item.type_name()
        )))
    }
}

pub(crate) fn item_has_key(item: &ItemRs, key: &str) -> PyResult<bool> {
    if let Some(table) = item.as_table() {
        Ok(table.contains_key(key))
    } else if let Some(it) = item.as_inline_table() {
        Ok(it.contains_key(key))
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' has no get()",
            item.type_name()
        )))
    }
}

// ---------------------------------------------------------------------------
// Setitem
// ---------------------------------------------------------------------------

pub(crate) fn item_setitem(item: &mut ItemRs, key: &Bound<'_, PyAny>, value: Item) -> PyResult<()> {
    match item {
        ItemRs::Value(value_rs) => match value_rs {
            toml_edit::Value::Array(array) => {
                let v = value
                    .0
                    .into_value()
                    .map_err(|_| PyTypeError::new_err("Item cannot be assigned to array"))?;
                let idx = resolve_index(key.extract::<i64>()?, array.len())?;
                array.replace(idx, v);
                Ok(())
            }
            toml_edit::Value::InlineTable(inline_table) => {
                let mut v = value
                    .0
                    .into_value()
                    .map_err(|_| PyTypeError::new_err("Item cannot be assigned to inline table"))?;
                let key: &str = key.extract()?;
                if let Some(old) = inline_table.get(key) {
                    let old_decor = old.decor();
                    if let Some(prefix) = old_decor.prefix() {
                        v.decor_mut().set_prefix(prefix.clone());
                    }
                    if let Some(suffix) = old_decor.suffix() {
                        v.decor_mut().set_suffix(suffix.clone());
                    }
                }
                inline_table[key] = v;
                Ok(())
            }
            _ => {
                let name = value_rs.type_name();
                Err(PyTypeError::new_err(format!(
                    "'{name}' is not subscriptable"
                )))
            }
        },
        ItemRs::Table(_) => {
            let key: &str = key.extract()?;
            set_with_decor_preservation(item, key, value);
            Ok(())
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

pub(crate) fn item_delitem(item: &mut ItemRs, key: &Bound<'_, PyAny>) -> PyResult<()> {
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

pub(crate) fn item_pop(item: &mut ItemRs, key: Option<&Bound<'_, PyAny>>) -> PyResult<Item> {
    match key {
        Some(key_obj) => {
            if let Some(table) = item.as_table_mut() {
                let key: &str = key_obj.extract()?;
                table
                    .remove(key)
                    .map(Item)
                    .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
            } else if let Some(it) = item.as_inline_table_mut() {
                let key: &str = key_obj.extract()?;
                it.remove(key)
                    .map(|v| Item(ItemRs::Value(v)))
                    .ok_or_else(|| PyKeyError::new_err(key.to_owned()))
            } else if let Some(arr) = item.as_array_mut() {
                let idx = resolve_index(key_obj.extract::<i64>()?, arr.len())?;
                Ok(Item(ItemRs::Value(arr.remove(idx))))
            } else {
                Err(PyTypeError::new_err(format!(
                    "'{}' does not support pop()",
                    item.type_name()
                )))
            }
        }
        None => {
            if let Some(arr) = item.as_array_mut() {
                if arr.is_empty() {
                    return Err(PyIndexError::new_err("pop from empty array"));
                }
                let last = arr.len() - 1;
                Ok(Item(ItemRs::Value(arr.remove(last))))
            } else {
                Err(PyTypeError::new_err(
                    "pop() with no arguments only supported on arrays",
                ))
            }
        }
    }
}

pub(crate) fn item_update(item: &mut ItemRs, other: &Bound<'_, PyAny>) -> PyResult<()> {
    let pairs: Vec<(String, Item)> = if let Ok(dict) = other.cast::<PyDict>() {
        dict.iter()
            .map(|(k, v)| {
                let key: String = k.extract()?;
                let val: Item = v.extract()?;
                Ok((key, val))
            })
            .collect::<PyResult<_>>()?
    } else {
        return Err(PyTypeError::new_err("update() argument must be a dict"));
    };

    if item.is_table() || item.is_inline_table() {
        for (key, value) in pairs {
            set_with_decor_preservation(item, &key, value);
        }
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support update()",
            item.type_name()
        )))
    }
}

// ---------------------------------------------------------------------------
// Mutation: list-like
// ---------------------------------------------------------------------------

pub(crate) fn item_append(item: &mut ItemRs, value: Item) -> PyResult<()> {
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

pub(crate) fn item_insert(item: &mut ItemRs, index: usize, value: Item) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        if index > arr.len() {
            return Err(PyIndexError::new_err("insert index out of range"));
        }
        let v = value
            .0
            .into_value()
            .map_err(|_| PyTypeError::new_err("cannot insert non-value into array"))?;
        arr.insert(index, v);
        Ok(())
    } else {
        Err(PyTypeError::new_err(format!(
            "'{}' does not support insert()",
            item.type_name()
        )))
    }
}

pub(crate) fn item_remove(item: &mut ItemRs, value: &Bound<'_, PyAny>) -> PyResult<()> {
    if let Some(arr) = item.as_array_mut() {
        for i in 0..arr.len() {
            if let Some(v) = arr.get(i)
                && value_eq(v, value)?
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

pub(crate) fn item_extend(item: &mut ItemRs, items: Vec<Item>) -> PyResult<()> {
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

pub(crate) fn item_clear(item: &mut ItemRs) -> PyResult<()> {
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
