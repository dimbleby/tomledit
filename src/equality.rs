use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDateTime, PyDict, PyList, PyString};
use toml_edit::Item as ItemRs;

use crate::value::Datetime;

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

/// Compare two toml_edit Values structurally (pure Rust, no Python allocation).
fn values_structural_eq(a: &toml_edit::Value, b: &toml_edit::Value) -> bool {
    if let (Some(a), Some(b)) = (a.as_bool(), b.as_bool()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (a.as_integer(), b.as_integer()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (a.as_float(), b.as_float()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (a.as_str(), b.as_str()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (a.as_datetime(), b.as_datetime()) {
        return datetime_eq(a, b);
    }
    if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
        return a.len() == b.len()
            && a.iter()
                .zip(b.iter())
                .all(|(va, vb)| values_structural_eq(va, vb));
    }
    if let (Some(a), Some(b)) = (a.as_inline_table(), b.as_inline_table()) {
        return a.len() == b.len()
            && a.iter()
                .all(|(k, v)| b.get(k).is_some_and(|bv| values_structural_eq(v, bv)));
    }
    false
}

fn tables_structural_eq(a: &toml_edit::Table, b: &toml_edit::Table) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(k, v)| b.get(k).is_some_and(|bv| items_structural_eq(v, bv)))
}

pub(crate) fn items_structural_eq(a: &ItemRs, b: &ItemRs) -> bool {
    match (a, b) {
        (ItemRs::Value(va), ItemRs::Value(vb)) => values_structural_eq(va, vb),
        (ItemRs::Table(ta), ItemRs::Table(tb)) => tables_structural_eq(ta, tb),
        (ItemRs::ArrayOfTables(aa), ItemRs::ArrayOfTables(ab)) => {
            aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab.iter())
                    .all(|(ta, tb)| tables_structural_eq(ta, tb))
        }
        _ => false,
    }
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
        && other.cast::<PyBool>().is_err()
        && let Ok(other_i) = other.extract::<i64>()
    {
        return Ok(i == other_i);
    }
    if let Some(f) = value.as_float()
        && other.cast::<PyBool>().is_err()
        && let Ok(other_f) = other.extract::<f64>()
    {
        return Ok(f == other_f);
    }
    if let Some(s) = value.as_str()
        && let Ok(other_s) = other.cast::<PyString>()
    {
        return Ok(other_s.to_str().is_ok_and(|o| s == o));
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
pub(crate) fn table_entries_eq<'a>(
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
