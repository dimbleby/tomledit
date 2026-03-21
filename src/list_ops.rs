use pyo3::prelude::*;
use pyo3::types::PyDict;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{apply_removal_decor, into_value, save_removal_decor, unsupported_op};

// ---------------------------------------------------------------------------
// Private helpers
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

fn require_table(item: Item) -> PyResult<toml_edit::Table> {
    match item.0 {
        ItemRs::Table(t) => Ok(t),
        ItemRs::Value(ValueRs::InlineTable(it)) => Ok(it.into_table()),
        other => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "cannot append {} to array of tables (expected a table/dict)",
            other.type_name()
        ))),
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

// ---------------------------------------------------------------------------
// List-like operations
// ---------------------------------------------------------------------------

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
        Err(pyo3::exceptions::PyValueError::new_err(
            "value not in array",
        ))
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
        Err(pyo3::exceptions::PyValueError::new_err(
            "value not in array",
        ))
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
            Err(pyo3::exceptions::PyValueError::new_err(
                "value not in array",
            ))
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
            Err(pyo3::exceptions::PyValueError::new_err(
                "value not in array",
            ))
        }
        _ => Err(unsupported_op(item, "index()")),
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
