use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyDict, PyString, PyTuple, PyType};
use toml_edit::{Item as ItemRs, TableLike, Value as ValueRs};

use crate::comments;
use crate::comments::CommentPreservation;
use crate::document::Document;
use crate::item::Item;
use crate::item_ops::{self, Key, unsupported_op};
use crate::item_proxy::with_proxy_or_doc_item;
use crate::py_pairs::extract_pair;

// ---------------------------------------------------------------------------
// Pop helpers
// ---------------------------------------------------------------------------

/// Validate and extract the optional default from `pop(key, /, *default)`.
pub(crate) fn extract_pop_default(default: &Bound<'_, PyTuple>) -> PyResult<Option<Py<PyAny>>> {
    if default.len() > 1 {
        return Err(PyTypeError::new_err(format!(
            "pop expected at most 2 arguments, got {}",
            1 + default.len()
        )));
    }
    if default.is_empty() {
        Ok(None)
    } else {
        Ok(Some(default.get_item(0)?.unbind()))
    }
}

// ---------------------------------------------------------------------------
// Inline-table comment-preserving helpers
// ---------------------------------------------------------------------------

/// Remove a key from an inline table, preserving sibling inline comments.
/// Returns the removed value, or `None` if the key was not found.
pub(crate) fn inline_table_remove(
    it: &mut toml_edit::InlineTable,
    key: &str,
) -> Option<toml_edit::Value> {
    let mut ic = it.save_inline_comments();
    let pos = it.iter().position(|(k, _)| k == key);
    // Removing the first or last key drops the structural whitespace that hugged
    // `{` or `}` (carried by the removed entry's leaf-decor prefix / value
    // suffix respectively).  The surviving edge entry keeps its own inner
    // separator whitespace, so without re-applying that edge whitespace a spaced
    // table renders `{a = 1 }` (first removed) or `{ a = 1}` (last removed).
    let first_prefix = (pos == Some(0)).then(|| {
        it.key(key)
            .and_then(|k| k.leaf_decor().prefix().and_then(|r| r.as_str()))
            .unwrap_or_default()
            .to_owned()
    });
    let last_suffix = (pos == Some(it.len() - 1) && it.len() > 1).then(|| {
        it.get(key)
            .and_then(comments::value_suffix)
            .map(|s| comments::value_suffix_structural(s).to_owned())
            .unwrap_or_default()
    });
    let removed = it.remove(key)?;
    if let Some(pos) = pos {
        ic.remove(pos);
    }
    it.restore_inline_comments(&ic);
    if let Some(prefix) = first_prefix {
        let new_first = it.iter().next().map(|(k, _)| k.to_owned());
        if let Some(mut km) = new_first.and_then(|k| it.key_mut(&k)) {
            km.leaf_decor_mut().set_prefix(prefix);
        }
    }
    if let Some(structural) = last_suffix {
        let new_last = it.iter().last().map(|(k, _)| k.to_owned());
        if let Some(v) = new_last.and_then(|k| it.get_mut(&k)) {
            // Only borrow the removed entry's structural whitespace when the
            // survivor has none of its own; a survivor that already carries a
            // break or before-`}` comment (e.g. leading-comma layout) keeps its
            // suffix, which already positions `}` correctly.
            let own = comments::value_suffix(v).unwrap_or_default();
            if !comments::suffix_holds_break(own) {
                v.decor_mut().set_suffix(structural);
            }
        }
    }
    Some(removed)
}

// ---------------------------------------------------------------------------
// Table removal helpers
// ---------------------------------------------------------------------------

/// Whether `key` is the first entry of `table`.
fn is_first_entry(table: &toml_edit::Table, key: &str) -> bool {
    table.iter().next().is_some_and(|(k, _)| k == key)
}

/// After removing the *first* entry from a table, strip the structural leading
/// `\n` from the new first header table/AoT — the first header in a document
/// (or parent table) carries no leading newline.  Callers must only invoke this
/// when the removed key was the first entry, so a non-first removal never
/// disturbs the survivor.
fn fix_first_header_prefix(table: &mut toml_edit::Table) {
    let Some(key) = table.iter().next().map(|(k, _)| k.to_owned()) else {
        return;
    };
    if let Some(decor) = table.get_mut(&key).and_then(item_ops::header_decor_mut) {
        item_ops::strip_leading_newline(decor);
    }
}

/// Remove `key` from `table`, restoring the "first header carries no leading
/// newline" invariant when the removed key was the first entry.  This is the
/// single removal path for standard tables — `__delitem__`, `pop`, and
/// `table_pop` all go through it.  (`popitem` removes the *last* entry, which
/// can never be the surviving first header, so it does not need this.)
pub(crate) fn remove_from_table(table: &mut toml_edit::Table, key: &str) -> Option<ItemRs> {
    let removing_first = is_first_entry(table, key);
    let removed = table.remove(key)?;
    if removing_first {
        fix_first_header_prefix(table);
    }
    Some(removed)
}

// ---------------------------------------------------------------------------
// Table-like extraction helpers
// ---------------------------------------------------------------------------

/// Extract a shared `TableLike` reference, or return a `TypeError`.
pub(crate) fn as_dict_like<'a>(item: &'a ItemRs, op: &str) -> PyResult<&'a dyn TableLike> {
    item.as_table_like().ok_or_else(|| unsupported_op(item, op))
}

// ---------------------------------------------------------------------------
// Decor preservation
// ---------------------------------------------------------------------------

pub(crate) fn set_with_decor_preservation(item: &mut ItemRs, key: &str, value: Item) {
    // Save the old block comment so we can restore it after replacement,
    // regardless of whether the comment storage location changes.
    let old_comment = comments::get_block_comment(item, key);

    // Tables and ArrayOfTables must stay as-is; into_value() would convert
    // a standard Table ([foo]) into an InlineTable (foo = {}).
    // Exception: inside inline tables, nested dicts MUST become inline tables.
    if (value.0.is_table() || value.0.is_array_of_tables()) && !item.is_inline_table() {
        let mut val = value.0;
        // Clear position so toml_edit applies its default ordering.
        // Decor (including comments) is kept — the restore at the end
        // will overwrite with the target's comment when one exists,
        // otherwise the source's comment comes through.
        if let Some(t) = val.as_table_mut() {
            t.set_position(None);
        }
        if let Some(aot) = val.as_array_of_tables_mut() {
            for t in aot.iter_mut() {
                t.set_position(None);
            }
        }
        item[key] = val;
        // Clear the old key's leaf_decor so that comments from a previous
        // scalar value don't leak into the output before the new header.
        if let Some(mut km) = item.as_table_mut().and_then(|t| t.key_mut(key)) {
            km.leaf_decor_mut().clear();
        }
        // A table/AoT that was first in its source document has no leading
        // `\n`.  When inserted after other entries, add the structural
        // newline so the header doesn't run into the preceding content.
        if let Some(table) = item.as_table_mut()
            && !is_first_entry(table, key)
            && let Some(decor) = table.get_mut(key).and_then(item_ops::header_decor_mut)
        {
            item_ops::ensure_leading_newline(decor);
        }
    } else {
        // For new keys in inline tables, preserve sibling inline comments
        // (existing keys don't change key order, so no save/restore needed).
        let inline_insertion = item
            .as_inline_table()
            .filter(|it| !it.contains_key(key))
            .map(|it| {
                let last = it.iter().last();
                // Capture the trailing whitespace that currently sits before
                // `}` (carried in the last value's suffix) so the new entry,
                // which becomes the last one, can inherit it instead of
                // falling back to toml_edit's default of a single space.
                let trailing_ws = last
                    .and_then(|(_, v)| comments::value_suffix(v))
                    .map(|s| comments::value_suffix_structural(s).to_owned());
                (
                    it.save_inline_comments(),
                    last.map(|(k, _)| k.to_owned()),
                    trailing_ws,
                )
            });

        let old_decor = item
            .get(key)
            .and_then(|e| e.as_value())
            .map(|v| v.decor().clone());

        // A header entry ([table]/[[aot]]) has no `=`, so its key leaf-decor
        // suffix between key and `=` is empty.  Replacing it with a value turns
        // it into a `key = value` entry, which needs the standard space.
        let was_header = item
            .get(key)
            .is_some_and(|e| e.is_table() || e.is_array_of_tables());

        // For brand-new keys in a regular table, copy the table's body
        // indent so the new key lines up with its siblings.
        let new_key_indent = old_decor
            .is_none()
            .then(|| item.as_table().map(crate::list_ops::table_body_indent))
            .flatten()
            .filter(|s| !s.is_empty());

        // into_value() only fails for Item::None which we never produce.
        let mut new_value = value
            .0
            .into_value()
            .expect("Item should be convertible to Value");
        if let Some(ref decor) = old_decor {
            if let Some(prefix) = decor.prefix() {
                new_value.decor_mut().set_prefix(prefix.clone());
            }
            if let Some(suffix) = decor.suffix() {
                new_value.decor_mut().set_suffix(suffix.clone());
            }
        }
        // The new entry becomes the last one in an inline table, so it inherits
        // the trailing whitespace that previously sat before `}`.  Without this
        // the value's unset suffix falls back to toml_edit's default of a
        // single space, adding spurious padding (e.g. `{a = 1, b = 2 }`).
        if let Some((_, _, Some(ws))) = inline_insertion.as_ref() {
            new_value.decor_mut().set_suffix(ws.clone());
        }
        item[key] = ItemRs::Value(new_value);

        if (was_header || new_key_indent.is_some())
            && let Some(mut km) = item.as_table_mut().and_then(|t| t.key_mut(key))
        {
            if was_header {
                km.leaf_decor_mut().set_suffix(" ");
            }
            if let Some(indent) = new_key_indent
                && km
                    .leaf_decor()
                    .prefix()
                    .and_then(|r| r.as_str())
                    .is_none_or(str::is_empty)
            {
                km.leaf_decor_mut().set_prefix(&indent);
            }
        }

        if let Some((mut ic, last_key, _)) = inline_insertion {
            ic.push(String::new());
            if let Some(it) = item.as_inline_table_mut() {
                if let Some(last_key) = last_key
                    && let Some(last) = it.get_mut(&last_key)
                {
                    last.decor_mut().set_suffix("");
                } else {
                    // The table was empty: drop the residual inter-brace gap
                    // (e.g. the `  ` of `{  }`) now sitting after the new value.
                    if let Some(trailing) = crate::list_ops::collapsed_empty_trailing(
                        it.trailing().as_str().unwrap_or_default(),
                    ) {
                        it.set_trailing(trailing);
                    }
                }
                it.restore_inline_comments(&ic);
                comments::align_inserted_inline_key(it, key);
            }
        }
    }

    // Restore the target's block comment only if the new item doesn't
    // carry one of its own (e.g. copied from another document).
    if comments::get_block_comment(item, key).is_none()
        && let Some(ref c) = old_comment
    {
        let _ = comments::set_block_comment(item, key, Some(c));
    }
}

// ---------------------------------------------------------------------------
// Key operations
// ---------------------------------------------------------------------------

/// Iterate over table keys without collecting into a Vec.
pub(crate) fn for_each_key(item: &ItemRs, mut f: impl FnMut(&str) -> PyResult<()>) -> PyResult<()> {
    let tbl = as_dict_like(item, "keys()")?;
    for (k, _) in tbl.iter() {
        f(k)?;
    }
    Ok(())
}

/// Iterate `(key, item)` pairs of a dict-like item, propagating errors.
pub(crate) fn for_each_entry(
    item: &ItemRs,
    mut f: impl FnMut(&str, &ItemRs) -> PyResult<()>,
) -> PyResult<()> {
    let tbl = as_dict_like(item, "items()")?;
    for (k, v) in tbl.iter() {
        f(k, v)?;
    }
    Ok(())
}

pub(crate) fn item_keys(item: &ItemRs) -> PyResult<Vec<String>> {
    let mut keys = Vec::new();
    for_each_key(item, |k| {
        keys.push(k.to_owned());
        Ok(())
    })?;
    Ok(keys)
}

pub(crate) fn item_has_key(item: &ItemRs, key: &str) -> PyResult<bool> {
    let tbl = item
        .as_table_like()
        .ok_or_else(|| unsupported_op(item, "key lookup"))?;
    Ok(tbl.contains_key(key))
}

/// Remove and return the last `(key, Item)` pair from a table-like item.
pub(crate) fn item_popitem(item: &mut ItemRs) -> PyResult<(String, ItemRs)> {
    // `popitem` removes the *last* entry, which can never be the surviving
    // first header, so the first-header fixup in `remove_from_table` is not
    // needed here.
    match item {
        ItemRs::Table(table) => {
            let k = table.iter().last().map(|(k, _)| k.to_owned());
            let k = k.ok_or_else(|| PyKeyError::new_err("popitem(): dictionary is empty"))?;
            let v = table.remove(&k).expect("key just found");
            Ok((k, v))
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let k = it.iter().last().map(|(k, _)| k.to_owned());
            let k = k.ok_or_else(|| PyKeyError::new_err("popitem(): dictionary is empty"))?;
            let v = ItemRs::Value(inline_table_remove(it, &k).expect("key just found"));
            Ok((k, v))
        }
        _ => Err(unsupported_op(item, "popitem()")),
    }
}

// ---------------------------------------------------------------------------
// Update helpers
// ---------------------------------------------------------------------------

/// Extract key-value pairs from a `PyDict`.
fn dict_to_pairs(dict: &Bound<'_, PyDict>) -> PyResult<Vec<(String, Item)>> {
    let mut pairs = Vec::with_capacity(dict.len());
    for (k, v) in dict.iter() {
        let key = item_ops::extract_key_str(&k)?
            .ok_or_else(|| PyTypeError::new_err("keys must be strings"))?;
        let val: Item = v.extract()?;
        pairs.push((key, val));
    }
    Ok(pairs)
}

/// Extract key-value pairs from a Python object for dict-like update.
///
/// Supports:
/// - `dict` objects (fast path)
/// - Mappings with a `.keys()` method
/// - Iterables of `(key, value)` pairs
pub(crate) fn extract_update_pairs(other: &Bound<'_, PyAny>) -> PyResult<Vec<(String, Item)>> {
    if let Ok(dict) = other.cast::<PyDict>() {
        return dict_to_pairs(dict);
    }

    // Mapping .items() or bare iterable of pairs — same extraction logic.
    let iter = if is_mapping_like(other)? {
        other.call_method0("items")?.try_iter()?
    } else {
        other.try_iter()?
    };
    let mut pairs = Vec::new();
    for item in iter {
        let (key, val) = extract_pair(&item?)?;
        let key = item_ops::extract_key_str(&key)?
            .ok_or_else(|| PyTypeError::new_err("keys must be strings"))?;
        pairs.push((key, val.extract::<Item>()?));
    }
    Ok(pairs)
}

/// Extract key-value pairs from `**kwargs`.
pub(crate) fn extract_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(String, Item)>> {
    kwargs.map_or_else(|| Ok(Vec::new()), dict_to_pairs)
}

/// Apply pre-extracted update pairs to an item.
///
/// Returns the keys that replaced existing entries.
pub(crate) fn apply_update_pairs(item: &mut ItemRs, pairs: Vec<(String, Item)>) -> Vec<String> {
    // Callers guarantee `item` is table-like; `item.get()` returns `None`
    // for non-table items, so the replaced-key check is safe regardless.
    let mut replaced_keys = Vec::new();
    for (key, val) in pairs {
        if item.get(&key).is_some() {
            replaced_keys.push(key.clone());
        }
        set_with_decor_preservation(item, &key, val);
    }
    replaced_keys
}

// ---------------------------------------------------------------------------
// TOML-level merge (preserves key decor / block comments)
// ---------------------------------------------------------------------------

/// Merge entries from `source` into `target` at the `toml_edit` level,
/// preserving key decorations (block comments) from the source for newly
/// inserted keys.  Conflicting keys use the source value (the target's
/// value decor is preserved).
///
/// Returns the list of keys that were overridden.
pub(crate) fn merge_table_entries(target: &mut ItemRs, source: &ItemRs) -> PyResult<Vec<String>> {
    let src = source
        .as_table_like()
        .ok_or_else(|| unsupported_op(source, "|"))?;
    let tgt = target
        .as_table_like()
        .ok_or_else(|| unsupported_op(target, "|"))?;

    // Collect source keys up-front to avoid borrow conflicts.
    let keys: Vec<String> = src.iter().map(|(k, _)| k.to_owned()).collect();
    // Pre-check which keys already exist in the target.
    let existed: Vec<bool> = keys.iter().map(|k| tgt.contains_key(k)).collect();

    let mut replaced = Vec::new();

    for (key, existed) in keys.into_iter().zip(existed) {
        if existed {
            replaced.push(key.clone());
        }

        // Clone the source value via the TableLike trait.  For inline tables
        // this wraps the Value in an Item automatically.
        let src_val = src.get(&key).unwrap().clone();
        set_with_decor_preservation(target, &key, Item(src_val));

        // For NEW keys, copy the source block comment via the comment API
        // rather than raw key decor.  That preserves the comment payload
        // without disturbing inline-table separator spacing.
        if !existed && let Some(comment) = comments::get_block_comment(source, &key) {
            comments::set_block_comment(target, &key, Some(&comment))?;
        }
    }

    Ok(replaced)
}

/// Pre-resolved update source, extracted before taking a write lock
/// on the target document.  This avoids lock conflicts when the
/// source contains proxies from the same document.
pub(crate) enum ResolvedUpdate {
    /// Source is a TOML-aware type (Document or `DictItem`), cloned at
    /// resolve time so no lock is needed during application.
    Toml(ItemRs),
    /// Source is a plain Python mapping or iterable of pairs.
    Pairs(Vec<(String, Item)>),
}

impl ResolvedUpdate {
    /// Apply this update to `target`, returning the keys that were replaced.
    pub(crate) fn apply(self, target: &mut ItemRs) -> PyResult<Vec<String>> {
        match self {
            Self::Toml(item) => merge_table_entries(target, &item),
            Self::Pairs(pairs) => Ok(apply_update_pairs(target, pairs)),
        }
    }
}

/// Resolve `other` into a [`ResolvedUpdate`], doing all Python object
/// access and document reads before the caller takes a write lock on the
/// target document.
pub(crate) fn resolve_update(other: &Bound<'_, PyAny>) -> PyResult<ResolvedUpdate> {
    if let Some(item) = with_proxy_or_doc_item(other, ItemRs::clone)? {
        Ok(ResolvedUpdate::Toml(item))
    } else {
        Ok(ResolvedUpdate::Pairs(extract_update_pairs(other)?))
    }
}

/// Merge `other` (a Python object) into `target`, dispatching to
/// [`merge_table_entries`] when `other` is a TOML-aware type (preserving
/// key decor / block comments) and falling back to [`extract_update_pairs`]
/// for plain Python mappings.
pub(crate) fn merge_other_into(
    target: &mut ItemRs,
    other: &Bound<'_, PyAny>,
) -> PyResult<Vec<String>> {
    if let Some(result) = with_proxy_or_doc_item(other, |item| merge_table_entries(target, item))? {
        return result;
    }
    // Plain mapping / iterable — no TOML decor to preserve.
    Ok(apply_update_pairs(target, extract_update_pairs(other)?))
}

/// Returns `true` if `other` is a `Mapping` (the `collections.abc` ABC),
/// or a TOML-aware mapping type (`Document` or `DictItem`).
pub(crate) fn is_mapping_like(other: &Bound<'_, PyAny>) -> PyResult<bool> {
    Ok(other.is_instance_of::<crate::dict_proxy::DictProxy>()
        || other.is_instance_of::<Document>()
        || other.is_instance_of::<PyDict>()
        || is_abc_mapping(other)?)
}

fn is_abc_mapping(obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    static MAPPING: PyOnceLock<Py<PyType>> = PyOnceLock::new();
    let cls = MAPPING.import(obj.py(), "collections.abc", "Mapping")?;
    obj.is_instance(cls.as_any())
}

/// Copy entries from a Python mapping into a new `PyDict`, preserving the
/// original Python values verbatim (no TOML round-trip).
///
/// Used by `__ror__` where the LHS is a plain mapping and the result must
/// be a plain dict — non-TOML values like `None` should pass through.
pub(crate) fn copy_mapping_to_pydict<'py>(
    other: &Bound<'py, PyAny>,
    py: Python<'py>,
) -> PyResult<Bound<'py, PyDict>> {
    if let Ok(dict) = other.cast::<PyDict>() {
        let result = PyDict::new(py);
        for (key, value) in dict.iter() {
            result.set_item(normalize_plain_dict_key(&key, py)?, value)?;
        }
        return Ok(result);
    }
    let dict = PyDict::new(py);
    let items = other.call_method0("items")?;
    for pair in items.try_iter()? {
        let pair = pair?;
        let (key, value) = extract_pair(&pair)?;
        dict.set_item(normalize_plain_dict_key(&key, py)?, value)?;
    }
    Ok(dict)
}

fn normalize_plain_dict_key<'py>(key: &Bound<'py, PyAny>, py: Python<'py>) -> PyResult<Py<PyAny>> {
    if let Some(key) = item_ops::extract_key_str(key)? {
        return Ok(PyString::new(py, &key).into_any().unbind());
    }
    Ok(key.clone().unbind())
}

/// Remove a key from a table-like item, returning the removed item and key.
pub(crate) fn table_pop(item: &mut ItemRs, key: &str) -> PyResult<(Item, Key)> {
    match item {
        ItemRs::Table(table) => remove_from_table(table, key)
            .map(|v| (Item(v), Key::Str(key.into())))
            .ok_or_else(|| PyKeyError::new_err(key.to_owned())),
        ItemRs::Value(ValueRs::InlineTable(it)) => inline_table_remove(it, key)
            .map(|v| (Item(ItemRs::Value(v)), Key::Str(key.into())))
            .ok_or_else(|| PyKeyError::new_err(key.to_owned())),
        _ => Err(unsupported_op(item, "pop()")),
    }
}

/// Set a string-keyed entry (table / inline table).
/// Returns `Some(key)` if an existing value was replaced, `None` if a new key
/// was added.
///
/// The caller must ensure `item` is a table or inline table.
pub(crate) fn item_setitem_str(item: &mut ItemRs, key: String, value: Item) -> Option<Key> {
    let replaced = item.get(key.as_str()).is_some();
    set_with_decor_preservation(item, &key, value);
    replaced.then_some(Key::Str(key))
}
