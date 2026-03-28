use pyo3::exceptions::{PyKeyError, PyTypeError};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use toml_edit::Item as ItemRs;
use toml_edit::TableLike;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::document::Document;
use crate::item::Item;
use crate::item_ops::{self, Key, unsupported_op};
use crate::item_proxy::ItemProxy;

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
    // into_value() only fails for Item::None which we never produce.
    let mut new_value = value
        .0
        .into_value()
        .expect("Item should be convertible to Value");
    if let Some(decor) = old_decor {
        if let Some(prefix) = decor.prefix() {
            new_value.decor_mut().set_prefix(prefix.clone());
        }
        if let Some(suffix) = decor.suffix() {
            new_value.decor_mut().set_suffix(suffix.clone());
        }
    }
    item[key] = ItemRs::Value(new_value);

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

/// Iterate over table keys without collecting into a Vec.
pub(crate) fn for_each_key(item: &ItemRs, mut f: impl FnMut(&str) -> PyResult<()>) -> PyResult<()> {
    let tbl = as_dict_like(item, "keys()")?;
    for (k, _) in tbl.iter() {
        f(k)?;
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
    if let Some(tbl) = item.as_table_like() {
        return Ok(tbl.contains_key(key));
    }
    match item {
        ItemRs::Value(ValueRs::Array(_)) | ItemRs::ArrayOfTables(_) => Err(PyTypeError::new_err(
            "TOML array indices must be integers, not strings",
        )),
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item is not subscriptable (use .value to get the Python object)",
            item.type_name()
        ))),
    }
}

/// Remove and return the last `(key, Item)` pair from a table-like item.
pub(crate) fn item_popitem(item: &mut ItemRs, py: Python<'_>) -> PyResult<(String, Py<PyAny>)> {
    let (key, removed) = match item {
        ItemRs::Table(table) => {
            let k = table.iter().last().map(|(k, _)| k.to_owned());
            let k = k.ok_or_else(|| PyKeyError::new_err("popitem(): table is empty"))?;
            let v = table.remove(&k).expect("key just found");
            (k, v)
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let k = it.iter().last().map(|(k, _)| k.to_owned());
            let k = k.ok_or_else(|| PyKeyError::new_err("popitem(): table is empty"))?;
            let v = ItemRs::Value(item_ops::it_remove(it, &k).expect("key just found"));
            (k, v)
        }
        _ => return Err(unsupported_op(item, "popitem()")),
    };
    let py_val = item_ops::item_to_py(&removed, py)?;
    Ok((key, py_val))
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

    // Mapping — iterate .items() for (key, value) pairs directly.
    if is_mapping_like(other) {
        let items = other.call_method0("items")?;
        let mut pairs = Vec::new();
        for item in items.try_iter()? {
            let (key, val): (String, Item) = item?.extract()?;
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

/// Extract key-value pairs from `**kwargs`.
pub(crate) fn extract_kwargs(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Vec<(String, Item)>> {
    let Some(kw) = kwargs else {
        return Ok(Vec::new());
    };
    let mut pairs = Vec::with_capacity(kw.len());
    for (k, v) in kw.iter() {
        let key: String = k.extract()?;
        let val: Item = v.extract()?;
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
    let mut replaced_keys = Vec::new();
    for (key, val) in pairs {
        let existed = as_dict_like(item, "update()")?.contains_key(&key);
        set_with_decor_preservation(item, &key, val);
        if existed {
            replaced_keys.push(key);
        }
    }
    Ok(replaced_keys)
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

        // For NEW keys in non-inline tables, copy the source key's decor
        // (block comments).  Inline tables handle their own separator
        // formatting, so copying decor would break spacing.
        if !existed
            && !target.is_value()
            && let Some((src_key, _)) = src.get_key_value(&key)
        {
            let decor = src_key.leaf_decor().clone();
            let tgt = target.as_table_like_mut().expect("target checked above");
            if let Some(mut km) = tgt.key_mut(&key) {
                if let Some(p) = decor.prefix() {
                    km.leaf_decor_mut().set_prefix(p.clone());
                }
                if let Some(s) = decor.suffix() {
                    km.leaf_decor_mut().set_suffix(s.clone());
                }
            }
        }
    }

    Ok(replaced)
}

/// Pre-resolved update source, extracted before taking a mutable borrow
/// on the target document.  This avoids double-borrow panics when the
/// source contains proxies from the same document.
pub(crate) enum ResolvedUpdate<'py> {
    /// Source is a TOML-aware type (Document or DictItem).
    Toml(TomlSource<'py>),
    /// Source is a plain Python mapping or iterable of pairs.
    Pairs(Vec<(String, Item)>),
}

impl ResolvedUpdate<'_> {
    /// Apply this update to `target`, returning the keys that were replaced.
    pub(crate) fn apply(self, target: &mut ItemRs) -> PyResult<Vec<String>> {
        match self {
            Self::Toml(src) => merge_table_entries(target, src.as_item()?),
            Self::Pairs(pairs) => apply_update_pairs(target, pairs),
        }
    }
}

/// Resolve `other` into a [`ResolvedUpdate`], doing all Python object
/// access before the caller takes a mutable borrow on the target document.
pub(crate) fn resolve_update<'py>(
    other: &Bound<'py, PyAny>,
    self_doc: &Bound<'py, Document>,
) -> PyResult<ResolvedUpdate<'py>> {
    match resolve_toml_source(other, self_doc)? {
        Some(src) => Ok(ResolvedUpdate::Toml(src)),
        None => Ok(ResolvedUpdate::Pairs(extract_update_pairs(other)?)),
    }
}

/// A TOML source resolved for merging, holding any necessary borrow guards.
///
/// Callers obtain this via [`resolve_toml_source`], then call [`.as_item()`]
/// to get the `&ItemRs` reference.  The `PyRef` guard keeps the source
/// document borrowed so the reference remains valid while the caller mutates
/// its own document.
pub(crate) enum TomlSource<'py> {
    /// Source is from a different document — borrow guard kept alive, path
    /// navigated on demand (empty path = document root).
    Borrowed {
        doc_ref: PyRef<'py, Document>,
        path: Vec<Key>,
    },
    /// Source is from the same document — had to clone.
    Owned(ItemRs),
}

impl TomlSource<'_> {
    pub(crate) fn as_item(&self) -> PyResult<&ItemRs> {
        match self {
            Self::Borrowed { doc_ref, path } => item_ops::navigate_path(&doc_ref.inner, path),
            Self::Owned(item) => Ok(item),
        }
    }
}

/// Resolve a TOML-aware source for merging.
///
/// When `other` is an [`ItemProxy`] or [`Document`], returns a
/// [`TomlSource`] that borrows the underlying item zero-copy — **unless**
/// the source shares the same document as `self_doc`, in which case it
/// clones to avoid a double-borrow.
///
/// Returns `None` when `other` is a plain Python object.
pub(crate) fn resolve_toml_source<'py>(
    other: &Bound<'py, PyAny>,
    self_doc: &Bound<'py, Document>,
) -> PyResult<Option<TomlSource<'py>>> {
    if let Ok(proxy) = other.cast::<ItemProxy>() {
        let proxy_ref = proxy.borrow();
        let doc_bound = proxy_ref.document.bind(other.py());
        let doc_ref = doc_bound.borrow();
        proxy_ref.check_fresh(&doc_ref)?;
        if doc_bound.is(self_doc) {
            let item = proxy_ref.navigate(&doc_ref.inner)?.clone();
            return Ok(Some(TomlSource::Owned(item)));
        }
        let path = proxy_ref.path.clone();
        return Ok(Some(TomlSource::Borrowed { doc_ref, path }));
    }
    if let Ok(doc_bound) = other.cast::<Document>() {
        if doc_bound.is(self_doc) {
            let item = doc_bound.borrow().inner.as_item().clone();
            return Ok(Some(TomlSource::Owned(item)));
        }
        return Ok(Some(TomlSource::Borrowed {
            doc_ref: doc_bound.borrow(),
            path: Vec::new(),
        }));
    }
    Ok(None)
}

/// Merge `other` (a Python object) into `target`, dispatching to
/// [`merge_table_entries`] when `other` is a TOML-aware type (preserving
/// key decor / block comments) and falling back to [`extract_update_pairs`]
/// for plain Python mappings.
pub(crate) fn merge_other_into(
    target: &mut ItemRs,
    other: &Bound<'_, PyAny>,
    py: Python<'_>,
) -> PyResult<Vec<String>> {
    if let Ok(proxy) = other.cast::<ItemProxy>() {
        let proxy = proxy.borrow();
        let other_doc = proxy.document.bind(py).borrow();
        proxy.check_fresh(&other_doc)?;
        let other_item = proxy.navigate(&other_doc.inner)?;
        return merge_table_entries(target, other_item);
    }
    if let Ok(doc_bound) = other.cast::<Document>() {
        let doc = doc_bound.borrow();
        return merge_table_entries(target, doc.inner.as_item());
    }
    // Plain mapping / iterable — no TOML decor to preserve.
    apply_update_pairs(target, extract_update_pairs(other)?)
}

/// Returns `true` if `other` is a `Mapping` (the `collections.abc` ABC),
/// or a TOML-aware type (`ItemProxy` or `Document`).
pub(crate) fn is_mapping_like(other: &Bound<'_, PyAny>) -> bool {
    other.is_instance_of::<ItemProxy>()
        || other.is_instance_of::<Document>()
        || other.is_instance_of::<PyDict>()
        || is_abc_mapping(other)
}

fn is_abc_mapping(obj: &Bound<'_, PyAny>) -> bool {
    let py = obj.py();
    py.import("collections.abc")
        .and_then(|m| m.getattr("Mapping"))
        .and_then(|cls| obj.is_instance(&cls))
        .unwrap_or(false)
}
