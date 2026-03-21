use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::item::Item;
use crate::item_ops::unsupported_op;

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
// Key operations
// ---------------------------------------------------------------------------

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
// Update helpers
// ---------------------------------------------------------------------------

/// Extract key-value pairs from a Python object for dict-like update.
///
/// Supports:
/// - `dict` objects (fast path)
/// - Mappings with a `.keys()` method
/// - Iterables of `(key, value)` pairs
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
/// Returns the keys that replaced existing entries.
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
