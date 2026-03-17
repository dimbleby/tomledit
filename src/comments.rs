use pyo3::exceptions::{PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

/// Extract an inline comment from a raw string: trim whitespace, return if `#`-prefixed.
fn extract_inline_comment(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.starts_with('#') {
        Some(trimmed.to_owned())
    } else {
        None
    }
}

/// Extract block comment lines from a raw string.
/// Returns `#`-prefixed lines (trimmed of indentation) and empty lines for blank lines.
/// Returns `None` if there are no comment lines.
fn extract_block_comment(s: &str) -> Option<String> {
    let mut result: Vec<&str> = Vec::new();
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            result.push(trimmed);
        } else if trimmed.is_empty() {
            result.push("");
        }
    }
    if result.iter().all(|s| s.is_empty()) {
        None
    } else {
        Some(result.join("\n"))
    }
}

/// Validate an inline comment and format it for storage in a decor suffix.
pub(crate) fn validate_inline_comment(text: &str) -> PyResult<String> {
    if text.contains('\n') {
        return Err(PyValueError::new_err(
            "inline comment must not contain newlines",
        ));
    }
    if !text.starts_with('#') {
        return Err(PyValueError::new_err("comment must start with '#'"));
    }
    Ok(format!(" {text}"))
}

/// Build a block comment string from user text.
/// Uses `split('\n')`: non-empty lines must start with `#`, empty lines become blank lines.
/// Each `#`-prefixed line is indented with `indent`.
fn build_block_comment(text: &str, indent: &str) -> PyResult<String> {
    let mut out = String::new();
    for l in text.split('\n') {
        if l.is_empty() {
            out.push('\n');
        } else if l.starts_with('#') {
            out.push_str(indent);
            out.push_str(l);
            out.push('\n');
        } else {
            return Err(PyValueError::new_err("comment lines must start with '#'"));
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Key-value comment operations
// ---------------------------------------------------------------------------

/// Get the inline comment (suffix) from an item's value decor.
pub(crate) fn get_suffix_comment(item: &ItemRs) -> Option<String> {
    let decor = match item {
        ItemRs::Value(v) => v.decor(),
        ItemRs::Table(t) => t.decor(),
        _ => return None,
    };
    let raw = decor.suffix()?.as_str()?;
    extract_inline_comment(raw)
}

/// Set the inline comment (suffix) on an item's value decor.
pub(crate) fn set_suffix_comment(item: &mut ItemRs, comment: Option<&str>) -> PyResult<()> {
    let decor = match item {
        ItemRs::Value(v) => v.decor_mut(),
        ItemRs::Table(t) => t.decor_mut(),
        _ => {
            return Err(PyTypeError::new_err(format!(
                "'{}' does not support comments",
                item.type_name()
            )));
        }
    };
    match comment {
        Some(text) => decor.set_suffix(validate_inline_comment(text)?),
        None => decor.set_suffix(""),
    }
    Ok(())
}

/// Get the comment before a key from the parent table's key decor.
///
/// For standard tables (`[name]`), the block comment lives in the child
/// Table's own decor prefix (before the `[` bracket), not in the key's
/// leaf decor (which would be *inside* the brackets).
pub(crate) fn get_key_prefix_comment(parent: &ItemRs, key: &str) -> Option<String> {
    if let ItemRs::Table(table) = parent
        && let Some(ItemRs::Table(child)) = table.get(key)
    {
        let raw = child.decor().prefix()?.as_str()?;
        return extract_block_comment(raw);
    }
    if let ItemRs::Value(ValueRs::InlineTable(it)) = parent {
        return get_it_block_comment(it, key);
    }
    let raw = match parent {
        ItemRs::Table(table) => table.key(key)?.leaf_decor().prefix()?.as_str()?,
        _ => return None,
    };
    extract_block_comment(raw)
}

/// Set the comment before a key in the parent table's key decor.
///
/// For standard tables (`[name]`), the block comment is stored in the child
/// Table's own decor prefix (before the `[` bracket).  For plain key-value
/// pairs the comment lives in the key's leaf decor prefix.
pub(crate) fn set_key_prefix_comment(
    parent: &mut ItemRs,
    key: &str,
    comment: Option<&str>,
) -> PyResult<()> {
    if let ItemRs::Table(table) = parent
        && table.get(key).is_some_and(|item| item.is_table())
    {
        let child = table.get_mut(key).unwrap().as_table_mut().unwrap();
        match comment {
            Some(text) => child.decor_mut().set_prefix(build_block_comment(text, "")?),
            None => child.decor_mut().set_prefix("\n"),
        }
        return Ok(());
    }
    if let ItemRs::Value(ValueRs::InlineTable(it)) = parent {
        return set_it_block_comment(it, key, comment);
    }
    let key_mut = match parent {
        ItemRs::Table(table) => table.key_mut(key),
        _ => None,
    };
    let Some(mut km) = key_mut else {
        return Err(PyKeyError::new_err(key.to_owned()));
    };
    match comment {
        Some(text) => km
            .leaf_decor_mut()
            .set_prefix(build_block_comment(text, "")?),
        None => km.leaf_decor_mut().set_prefix(""),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Array element prefix decomposition
// ---------------------------------------------------------------------------
//
// In toml_edit an array element's inline comment is serialised *after* the
// comma but *before* the next value, so it lives in the **next** element's
// decor prefix (or the array trailing for the last element).  We call this
// location the element's "slot".
//
// A slot string has the form `{inline}\n{block}{indent}` where:
//   - `inline` is ` # text` (inline comment after comma) or empty
//   - `block`  is zero or more `{indent}# text\n` lines
//   - `indent` is the whitespace before the value
//
// The helpers below decompose / reconstruct this format and abstract over
// the "next-element-or-trailing" indirection so that callers can work in
// terms of *element index* without worrying about where the raw string
// physically lives.

struct PrefixParts {
    inline: String,
    block: String,
    indent: String,
}

fn split_prefix(raw: &str) -> PrefixParts {
    if let Some((first_line, rest)) = raw.split_once('\n') {
        if let Some((block, indent)) = rest.rsplit_once('\n') {
            PrefixParts {
                inline: first_line.to_owned(),
                block: if block.is_empty() {
                    String::new()
                } else {
                    format!("{block}\n")
                },
                indent: indent.to_owned(),
            }
        } else {
            PrefixParts {
                inline: first_line.to_owned(),
                block: String::new(),
                indent: rest.to_owned(),
            }
        }
    } else {
        PrefixParts {
            inline: String::new(),
            block: String::new(),
            indent: raw.to_owned(),
        }
    }
}

fn join_prefix(parts: &PrefixParts) -> String {
    format!("{}\n{}{}", parts.inline, parts.block, parts.indent)
}

// -- thin helpers over split_prefix / join_prefix --

/// Extract the inline-comment portion from a raw prefix or trailing string.
fn read_inline(raw: &str) -> String {
    split_prefix(raw).inline
}

/// Return `raw` with its inline-comment portion replaced by `new_inline`.
fn replace_inline(raw: &str, new_inline: &str) -> String {
    let mut parts = split_prefix(raw);
    parts.inline = new_inline.to_owned();
    join_prefix(&parts)
}

// -- slot helpers (abstract over "next element prefix vs trailing") --

/// Read the raw string from the inline-comment slot for array element `idx`.
fn slot_raw(arr: &toml_edit::Array, idx: usize) -> String {
    if idx + 1 < arr.len() {
        arr.get(idx + 1)
            .and_then(|v| v.decor().prefix().and_then(|r| r.as_str()))
            .unwrap_or_default()
            .to_owned()
    } else {
        arr.trailing().as_str().unwrap_or_default().to_owned()
    }
}

/// Write a raw string to the inline-comment slot for array element `idx`.
fn set_slot_raw(arr: &mut toml_edit::Array, idx: usize, raw: &str) {
    if idx + 1 < arr.len() {
        arr.get_mut(idx + 1).unwrap().decor_mut().set_prefix(raw);
    } else {
        arr.set_trailing(raw);
    }
}

// ---------------------------------------------------------------------------
// Array element comment operations
// ---------------------------------------------------------------------------

/// Get the inline comment for array element `idx`.
pub(crate) fn get_array_item_comment(parent: &ItemRs, idx: usize) -> Option<String> {
    let array = parent.as_value()?.as_array()?;
    if idx >= array.len() {
        return None;
    }
    extract_inline_comment(&read_inline(&slot_raw(array, idx)))
}

/// Set the inline comment for array element `idx`, preserving any block
/// comments.  `inline` is a pre-validated raw suffix (e.g. `" # note"`)
/// or empty to clear.
pub(crate) fn set_array_item_comment(arr: &mut toml_edit::Array, idx: usize, inline: &str) {
    set_slot_raw(arr, idx, &replace_inline(&slot_raw(arr, idx), inline));
}

/// Get the block comment before an array element from its value's decor prefix.
pub(crate) fn get_value_prefix_comment(item: &ItemRs) -> Option<String> {
    let decor = match item {
        ItemRs::Value(v) => v.decor(),
        _ => return None,
    };
    let raw = decor.prefix()?.as_str()?;
    let parts = split_prefix(raw);
    if parts.block.is_empty() {
        return None;
    }
    extract_block_comment(&parts.block)
}

/// Extract (and clear) an inline comment from a value's decor suffix.
///
/// When [`ItemProxy::clone_item`] clones an array element it copies the
/// slot-based inline comment into the value's decor suffix.  Array mutation
/// functions call this to retrieve that comment before pushing/replacing
/// the value, then feed it into the target array's slot system.
/// Returns the raw suffix (e.g. `" # note"`) or empty string if none,
/// matching the format used by [`save_inline_comments`].
pub(crate) fn take_inline_comment(value: &mut ValueRs) -> String {
    let raw = value
        .decor()
        .suffix()
        .and_then(|r| r.as_str())
        .unwrap_or_default();
    if raw.trim().starts_with('#') {
        let result = raw.to_owned();
        value.decor_mut().set_suffix("");
        result
    } else {
        String::new()
    }
}

/// Snapshot all inline comments in the array, indexed by element position.
pub(crate) fn save_inline_comments(arr: &toml_edit::Array) -> Vec<String> {
    (0..arr.len())
        .map(|i| read_inline(&slot_raw(arr, i)))
        .collect()
}

/// Restore inline comments after a mutation.
///
/// `comments` must have the same length as `arr` and be indexed by element
/// position.  Callers mirror their mutation on the `Vec` (e.g. `vec.insert`,
/// `vec.remove`, `vec.push`) before calling this so the mapping is trivial.
pub(crate) fn restore_inline_comments(arr: &mut toml_edit::Array, comments: &[String]) {
    debug_assert_eq!(comments.len(), arr.len());
    // Pre-compute updates (read all slots before writing any).
    let updates: Vec<Option<String>> = comments
        .iter()
        .enumerate()
        .map(|(i, inline)| {
            let raw = slot_raw(arr, i);
            if read_inline(&raw) != *inline {
                Some(replace_inline(&raw, inline))
            } else {
                None
            }
        })
        .collect();
    for (i, update) in updates.into_iter().enumerate() {
        if let Some(raw) = update {
            set_slot_raw(arr, i, &raw);
        }
    }
}

/// Set the block comment before an array element, preserving any inline comment
/// on the previous element and the indentation.
pub(crate) fn set_value_prefix_comment(item: &mut ItemRs, comment: Option<&str>) -> PyResult<()> {
    let decor = match item {
        ItemRs::Value(v) => v.decor_mut(),
        _ => {
            return Err(PyTypeError::new_err(format!(
                "'{}' does not support comment_before",
                item.type_name()
            )));
        }
    };
    let raw = decor.prefix().and_then(|r| r.as_str()).unwrap_or_default();
    let mut parts = split_prefix(raw);
    parts.block = match comment {
        Some(text) => build_block_comment(text, &parts.indent)?,
        None => String::new(),
    };
    let new_prefix = join_prefix(&parts);
    decor.set_prefix(new_prefix);
    Ok(())
}

// ---------------------------------------------------------------------------
// Inline-table comment operations
// ---------------------------------------------------------------------------
//
// Inline tables use the same next-element indirection as arrays: an inline
// comment after key K's value is stored in the *next* key's
// `leaf_decor().prefix()` (or `inline_table.trailing()` for the last key).
//
// Block comments (.comment) also live in the key's leaf_decor prefix, but
// for non-first keys the prefix mixes the *previous* key's inline comment
// with this key's block comment; `split_prefix` separates them.

// -- block comment helpers (.comment on inline table keys) --

/// Whether `key` is the first key in the inline table's iteration order.
fn is_first_it_key(it: &toml_edit::InlineTable, key: &str) -> bool {
    it.iter().next().is_some_and(|(k, _)| k == key)
}

/// Extract the block comment from an inline table key's prefix.
fn get_it_block_comment(it: &toml_edit::InlineTable, key: &str) -> Option<String> {
    let raw = it.key(key)?.leaf_decor().prefix()?.as_str()?;
    if is_first_it_key(it, key) {
        extract_block_comment(raw)
    } else {
        let parts = split_prefix(raw);
        if parts.block.is_empty() {
            None
        } else {
            extract_block_comment(&parts.block)
        }
    }
}

/// Set the block comment on an inline table key's prefix.
fn set_it_block_comment(
    it: &mut toml_edit::InlineTable,
    key: &str,
    comment: Option<&str>,
) -> PyResult<()> {
    let is_first = is_first_it_key(it, key);
    let mut km = it
        .key_mut(key)
        .ok_or_else(|| PyKeyError::new_err(key.to_owned()))?;
    if is_first {
        match comment {
            Some(text) => km
                .leaf_decor_mut()
                .set_prefix(build_block_comment(text, "")?),
            None => km.leaf_decor_mut().set_prefix(""),
        }
    } else {
        let raw = km
            .leaf_decor()
            .prefix()
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_owned();
        let mut parts = split_prefix(&raw);
        parts.block = match comment {
            Some(text) => build_block_comment(text, &parts.indent)?,
            None => String::new(),
        };
        km.leaf_decor_mut().set_prefix(join_prefix(&parts));
    }
    Ok(())
}

// -- slot system (inline comment in next key's prefix / trailing) --

/// Find the key that follows `key` in iteration order.
fn next_it_key(it: &toml_edit::InlineTable, key: &str) -> Option<String> {
    let mut iter = it.iter().skip_while(|(k, _)| *k != key);
    iter.next();
    iter.next().map(|(k, _)| k.to_owned())
}

/// Find the 0-based position of `key` in iteration order.
pub(crate) fn it_key_position(it: &toml_edit::InlineTable, key: &str) -> Option<usize> {
    it.iter().position(|(k, _)| k == key)
}

/// Read the raw slot string.  `next_key` is the key after the target in
/// iteration order, or `None` for the last key (which uses trailing).
fn it_slot_raw(it: &toml_edit::InlineTable, next_key: Option<&str>) -> String {
    match next_key {
        Some(next) => it
            .key(next)
            .and_then(|k| k.leaf_decor().prefix().and_then(|r| r.as_str()))
            .unwrap_or_default()
            .to_owned(),
        None => it.trailing().as_str().unwrap_or_default().to_owned(),
    }
}

/// Write a raw slot string.  `next_key` is the key after the target in
/// iteration order, or `None` for the last key (which uses trailing).
fn set_it_slot_raw(it: &mut toml_edit::InlineTable, next_key: Option<&str>, raw: &str) {
    match next_key {
        Some(next) => {
            it.key_mut(next).unwrap().leaf_decor_mut().set_prefix(raw);
        }
        None => it.set_trailing(raw),
    }
}

// -- higher-level inline table comment operations --

/// Get the inline comment for an inline-table key.
pub(crate) fn get_it_item_comment(it: &toml_edit::InlineTable, key: &str) -> Option<String> {
    it.get(key)?;
    let next = next_it_key(it, key);
    extract_inline_comment(&read_inline(&it_slot_raw(it, next.as_deref())))
}

/// Set the inline comment for an inline-table key.  `inline` is a
/// pre-validated raw suffix (e.g. `" # note"`) or empty to clear.
pub(crate) fn set_it_item_comment(it: &mut toml_edit::InlineTable, key: &str, inline: &str) {
    let next = next_it_key(it, key);
    let raw = it_slot_raw(it, next.as_deref());
    if read_inline(&raw) == inline {
        return;
    }
    set_it_slot_raw(it, next.as_deref(), &replace_inline(&raw, inline));
}

/// Snapshot all inline comments in the inline table (iteration order).
pub(crate) fn save_it_inline_comments(it: &toml_edit::InlineTable) -> Vec<String> {
    let keys: Vec<&str> = it.iter().map(|(k, _)| k).collect();
    (0..keys.len())
        .map(|i| read_inline(&it_slot_raw(it, keys.get(i + 1).copied())))
        .collect()
}

/// Restore inline-table inline comments after a mutation.
///
/// `comments` must have the same length as `it` and be in iteration order.
/// Callers mirror their mutation on the Vec before calling this.
pub(crate) fn restore_it_inline_comments(it: &mut toml_edit::InlineTable, comments: &[String]) {
    debug_assert_eq!(comments.len(), it.len());
    let keys: Vec<String> = it.iter().map(|(k, _)| k.to_owned()).collect();
    let n = keys.len();
    let updates: Vec<Option<String>> = (0..n)
        .map(|i| {
            let next = keys.get(i + 1).map(|k| k.as_str());
            let raw = it_slot_raw(it, next);
            if read_inline(&raw) != comments[i] {
                Some(replace_inline(&raw, &comments[i]))
            } else {
                None
            }
        })
        .collect();
    for (i, update) in updates.into_iter().enumerate() {
        if let Some(raw) = update {
            let next = keys.get(i + 1).map(|k| k.as_str());
            set_it_slot_raw(it, next, &raw);
        }
    }
}
