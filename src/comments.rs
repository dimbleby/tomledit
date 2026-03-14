use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
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
fn validate_inline_comment(text: &str) -> PyResult<String> {
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
    match parent {
        ItemRs::Table(table) => {
            if let Some(ItemRs::Table(child)) = table.get(key) {
                let raw = child.decor().prefix()?.as_str()?;
                extract_block_comment(raw)
            } else {
                let raw = table.key(key)?.leaf_decor().prefix()?.as_str()?;
                extract_block_comment(raw)
            }
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let raw = it.key(key)?.leaf_decor().prefix()?.as_str()?;
            extract_block_comment(raw)
        }
        _ => None,
    }
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
    // Block comments on inline-table keys would force the table onto
    // multiple lines, producing TOML that is invalid under TOML 1.0.
    if comment.is_some() && parent.is_inline_table() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "cannot set block comment on inline table key (would produce multi-line inline table)",
        ));
    }
    match parent {
        ItemRs::Table(table) => {
            let is_child_table = table.get(key).is_some_and(|item| item.is_table());
            if is_child_table {
                let child = table.get_mut(key).unwrap().as_table_mut().unwrap();
                match comment {
                    Some(text) => child.decor_mut().set_prefix(build_block_comment(text, "")?),
                    None => child.decor_mut().set_prefix("\n"),
                }
            } else {
                let Some(mut km) = table.key_mut(key) else {
                    return Err(PyKeyError::new_err(key.to_owned()));
                };
                match comment {
                    Some(text) => km
                        .leaf_decor_mut()
                        .set_prefix(build_block_comment(text, "")?),
                    None => km.leaf_decor_mut().set_prefix(""),
                }
            }
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => {
            let Some(mut km) = it.key_mut(key) else {
                return Err(PyKeyError::new_err(key.to_owned()));
            };
            match comment {
                Some(text) => km
                    .leaf_decor_mut()
                    .set_prefix(build_block_comment(text, "")?),
                None => km.leaf_decor_mut().set_prefix(""),
            }
        }
        _ => return Err(PyKeyError::new_err(key.to_owned())),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Array element prefix decomposition
// ---------------------------------------------------------------------------

/// Parts of an array element's prefix (or array trailing).
/// The prefix has the form: `{inline}\n{block}{indent}` where:
/// - `inline` is ` # text` (inline comment after comma) or empty
/// - `block` is zero or more `{indent}# text\n` lines
/// - `indent` is the whitespace before the value
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

// ---------------------------------------------------------------------------
// Array element comment operations
// ---------------------------------------------------------------------------

/// Get the raw prefix string for array element at `idx` (from the next
/// element's prefix, or the array trailing for the last element).
fn get_array_raw_prefix(array: &toml_edit::Array, idx: usize) -> Option<String> {
    let len = array.len();
    if idx >= len {
        return None;
    }
    if idx + 1 < len {
        array
            .get(idx + 1)?
            .decor()
            .prefix()?
            .as_str()
            .map(|s| s.to_owned())
    } else {
        array.trailing().as_str().map(|s| s.to_owned())
    }
}

/// Get the inline comment for array element `idx`.
pub(crate) fn get_array_item_comment(parent: &ItemRs, idx: usize) -> Option<String> {
    let array = parent.as_value()?.as_array()?;
    let raw = get_array_raw_prefix(array, idx)?;
    extract_inline_comment(&split_prefix(&raw).inline)
}

/// Set the inline comment for array element `idx`, preserving any block comments.
pub(crate) fn set_array_item_comment(
    parent: &mut ItemRs,
    idx: usize,
    comment: Option<&str>,
) -> PyResult<()> {
    let array = parent
        .as_value_mut()
        .and_then(|v| v.as_array_mut())
        .ok_or_else(|| PyTypeError::new_err("parent is not an array"))?;
    let len = array.len();
    if idx >= len {
        return Err(PyIndexError::new_err("array index out of range"));
    }
    let raw = get_array_raw_prefix(array, idx).unwrap_or_default();
    let mut parts = split_prefix(&raw);
    parts.inline = match comment {
        Some(text) => validate_inline_comment(text)?,
        None => String::new(),
    };
    let new_prefix = join_prefix(&parts);
    if let Some(elem) = array.get_mut(idx + 1) {
        elem.decor_mut().set_prefix(new_prefix);
    } else {
        array.set_trailing(new_prefix);
    }
    Ok(())
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
