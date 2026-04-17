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

/// Whether a character is forbidden in a TOML comment.
///
/// TOML comments allow tab (U+0009) and all characters from U+0020 onwards
/// except U+007F (DEL).  Control characters U+0000–U+0008 and U+000A–U+001F
/// are forbidden.
fn is_invalid_comment_char(c: char) -> bool {
    matches!(c, '\u{0000}'..='\u{0008}' | '\u{000A}'..='\u{001F}' | '\u{007F}')
}

/// Validate that `text` contains no characters forbidden in TOML comments.
fn validate_comment_text(text: &str) -> PyResult<()> {
    if let Some(c) = text.chars().find(|&c| is_invalid_comment_char(c)) {
        return Err(PyValueError::new_err(format!(
            "comment contains invalid character U+{:04X}",
            c as u32
        )));
    }
    Ok(())
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
    validate_comment_text(text)?;
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
            validate_comment_text(l)?;
            out.push_str(indent);
            out.push_str(l);
            out.push('\n');
        } else {
            return Err(PyValueError::new_err("comment lines must start with '#'"));
        }
    }
    Ok(out)
}

/// Build a block-comment string, or fall back to `default` when
/// `comment` is `None`.
fn build_block_or_default(comment: Option<&str>, indent: &str, default: &str) -> PyResult<String> {
    Ok(comment
        .map(|t| build_block_comment(t, indent))
        .transpose()?
        .unwrap_or_else(|| default.to_owned()))
}

// ---------------------------------------------------------------------------
// Key-value comment operations
// ---------------------------------------------------------------------------

/// Get the inline comment from an item's value decor (the `# ...` after the value).
pub(crate) fn get_inline_comment(item: &ItemRs) -> Option<String> {
    let decor = match item {
        ItemRs::Value(v) => v.decor(),
        ItemRs::Table(t) => t.decor(),
        _ => return None,
    };
    let raw = decor.suffix()?.as_str()?;
    extract_inline_comment(raw)
}

/// Set the inline comment on an item's value decor (the `# ...` after the value).
pub(crate) fn set_inline_comment(item: &mut ItemRs, comment: Option<&str>) -> PyResult<()> {
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

/// Get the block comment above a key in a table or inline table.
///
/// For standard tables (`[name]`) and arrays of tables (`[[name]]`), the
/// block comment lives in the child's own decor prefix (before the `[` or
/// `[[` bracket), not in the key's leaf decor (which would be *inside* the
/// brackets).
pub(crate) fn get_block_comment(parent: &ItemRs, key: &str) -> Option<String> {
    match parent {
        ItemRs::Table(table) => {
            let raw = match table.get(key)? {
                ItemRs::Table(child) => child.decor().prefix()?.as_str()?,
                ItemRs::ArrayOfTables(aot) => aot.iter().next()?.decor().prefix()?.as_str()?,
                _ => table.key(key)?.leaf_decor().prefix()?.as_str()?,
            };
            extract_block_comment(raw)
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => get_inline_table_block_comment(it, key),
        _ => None,
    }
}

/// Set the block comment above a key in a table or inline table.
///
/// For standard tables (`[name]`) and arrays of tables (`[[name]]`), the
/// block comment is stored in the child's own decor prefix (before the
/// `[` or `[[` bracket).  For plain key-value pairs the comment lives in
/// the key's leaf decor prefix.
pub(crate) fn set_block_comment(
    parent: &mut ItemRs,
    key: &str,
    comment: Option<&str>,
) -> PyResult<()> {
    match parent {
        ItemRs::Table(table) => {
            let decor = match table.get_mut(key) {
                Some(ItemRs::Table(child)) => Some(child.decor_mut()),
                Some(ItemRs::ArrayOfTables(aot)) => {
                    aot.iter_mut().next().map(toml_edit::Table::decor_mut)
                }
                _ => None,
            };
            if let Some(d) = decor {
                d.set_prefix(build_block_or_default(comment, "", "\n")?);
                return Ok(());
            }
            let Some(mut km) = table.key_mut(key) else {
                return Err(PyKeyError::new_err(key.to_owned()));
            };
            km.leaf_decor_mut()
                .set_prefix(build_block_or_default(comment, "", "")?);
            Ok(())
        }
        ItemRs::Value(ValueRs::InlineTable(it)) => set_inline_table_block_comment(it, key, comment),
        _ => Err(PyKeyError::new_err(key.to_owned())),
    }
}

// ---------------------------------------------------------------------------
// Element comment system — arrays and inline tables
// ---------------------------------------------------------------------------
//
// A "slot" is where toml_edit physically stores an element's inline comment.
// Because the comment sits after the comma but before the next value, it ends
// up in the **next** element's decor prefix (or the container's trailing
// string for the last element).  `slot(N)` means "the storage location that
// carries element N's inline comment" — not element N's own decor.
//
// A slot string has the form `{inline}\n{block}{indent}` where:
//   - `inline` is ` # text` (inline comment after comma) or empty
//   - `block`  is zero or more `{indent}# text\n` lines
//   - `indent` is the whitespace before the value
//
// Both arrays and inline tables use this same indirection.  The
// `CommentPreservation` trait abstracts over the difference so that
// save/restore operations are written once.

/// Decomposed parts of an element's slot string: `{inline}\n{block}{indent}`.
struct PrefixParts {
    inline: String,
    block: String,
    indent: String,
}

impl PrefixParts {
    /// Decompose a raw slot string into its inline, block, and indent parts.
    fn split(raw: &str) -> Self {
        if let Some((first_line, rest)) = raw.split_once('\n') {
            if let Some((block, indent)) = rest.rsplit_once('\n') {
                Self {
                    inline: first_line.to_owned(),
                    block: if block.is_empty() {
                        String::new()
                    } else {
                        format!("{block}\n")
                    },
                    indent: indent.to_owned(),
                }
            } else {
                Self {
                    inline: first_line.to_owned(),
                    block: String::new(),
                    indent: rest.to_owned(),
                }
            }
        } else {
            Self {
                inline: String::new(),
                block: String::new(),
                indent: raw.to_owned(),
            }
        }
    }

    /// Reconstruct the raw prefix string from its parts.
    fn join(&self) -> String {
        format!("{}\n{}{}", self.inline, self.block, self.indent)
    }

    /// Return `raw` with its inline-comment portion replaced by `new_inline`.
    fn with_inline(raw: &str, new_inline: &str) -> String {
        let mut parts = Self::split(raw);
        parts.inline = new_inline.to_owned();
        parts.join()
    }
}

// -- CommentPreservation trait and implementations --

/// Batch save/restore of element inline comments across mutations.
///
/// Arrays and inline tables store inline comments in the slot system
/// (see the section comment above).  This trait lets mutation code
/// snapshot and restore those comments without knowing the container type.
pub(crate) trait CommentPreservation {
    fn save_inline_comments(&self) -> Vec<String>;
    fn restore_inline_comments(&mut self, comments: &[String]);
}

// -- Array helpers --

/// Read the slot for array element `idx`: the next element's prefix,
/// or the trailing string for the last element.
fn array_read_slot(arr: &toml_edit::Array, idx: usize) -> &str {
    if idx + 1 < arr.len() {
        arr.get(idx + 1)
            .and_then(|v| v.decor().prefix().and_then(|r| r.as_str()))
            .unwrap_or_default()
    } else {
        arr.trailing().as_str().unwrap_or_default()
    }
}

/// Write the slot for array element `idx`.
fn array_write_slot(arr: &mut toml_edit::Array, idx: usize, raw: &str) {
    if idx + 1 < arr.len() {
        arr.get_mut(idx + 1).unwrap().decor_mut().set_prefix(raw);
    } else {
        arr.set_trailing(raw);
    }
}

impl CommentPreservation for toml_edit::Array {
    fn save_inline_comments(&self) -> Vec<String> {
        (0..self.len())
            .map(|i| PrefixParts::split(array_read_slot(self, i)).inline)
            .collect()
    }

    fn restore_inline_comments(&mut self, comments: &[String]) {
        debug_assert_eq!(comments.len(), self.len());
        for (i, inline) in comments.iter().enumerate() {
            let raw = array_read_slot(self, i);
            if PrefixParts::split(raw).inline != *inline {
                let new_raw = PrefixParts::with_inline(raw, inline);
                array_write_slot(self, i, &new_raw);
            }
        }
    }
}

// -- InlineTable helpers --

/// Read the slot for an inline-table element, given the next key name
/// (or `None` for the last element).
fn it_read_slot<'a>(it: &'a toml_edit::InlineTable, next_key: Option<&str>) -> &'a str {
    match next_key {
        Some(k) => it
            .key(k)
            .and_then(|k| k.leaf_decor().prefix().and_then(|r| r.as_str()))
            .unwrap_or_default(),
        None => it.trailing().as_str().unwrap_or_default(),
    }
}

/// Write the slot for an inline-table element, given the next key name.
fn it_write_slot(it: &mut toml_edit::InlineTable, next_key: Option<&str>, raw: &str) {
    match next_key {
        Some(k) => it.key_mut(k).unwrap().leaf_decor_mut().set_prefix(raw),
        None => it.set_trailing(raw),
    }
}

impl CommentPreservation for toml_edit::InlineTable {
    fn save_inline_comments(&self) -> Vec<String> {
        let keys: Vec<&str> = self.iter().map(|(k, _)| k).collect();
        (0..keys.len())
            .map(|i| PrefixParts::split(it_read_slot(self, keys.get(i + 1).copied())).inline)
            .collect()
    }

    fn restore_inline_comments(&mut self, comments: &[String]) {
        debug_assert_eq!(comments.len(), self.len());
        let keys: Vec<String> = self.iter().map(|(k, _)| k.to_owned()).collect();
        for (i, inline) in comments.iter().enumerate() {
            let next_key = keys.get(i + 1).map(String::as_str);
            let raw = it_read_slot(self, next_key);
            if PrefixParts::split(raw).inline != *inline {
                let new_raw = PrefixParts::with_inline(raw, inline);
                it_write_slot(self, next_key, &new_raw);
            }
        }
    }
}

// -- Single-element comment access --

/// Get the inline comment for array element `idx` from a parent `ItemRs`.
pub(crate) fn get_array_inline_comment(parent: &ItemRs, idx: usize) -> Option<String> {
    let array = parent.as_value()?.as_array()?;
    extract_inline_comment(&PrefixParts::split(array_read_slot(array, idx)).inline)
}

/// Set the inline comment for array element `idx`.  `inline` is a
/// pre-validated raw suffix (e.g. `" # note"`) or empty to clear.
pub(crate) fn set_array_inline_comment(array: &mut toml_edit::Array, idx: usize, inline: &str) {
    let raw = array_read_slot(array, idx).to_owned();
    let current = PrefixParts::split(&raw).inline;
    if current == inline {
        return;
    }
    array_write_slot(array, idx, &PrefixParts::with_inline(&raw, inline));
}

/// Get the inline comment for an inline-table entry by key name.
pub(crate) fn get_inline_table_inline_comment(
    it: &toml_edit::InlineTable,
    key: &str,
) -> Option<String> {
    let mut iter = it.iter().skip_while(|(k, _)| *k != key);
    // If the key doesn't exist, skip_while exhausts the iterator.
    iter.next()?;
    let next_key = iter.next().map(|(k, _)| k);
    extract_inline_comment(&PrefixParts::split(it_read_slot(it, next_key)).inline)
}

/// Set the inline comment for an inline-table entry by key name.
/// `inline` is a pre-validated raw suffix (e.g. `" # note"`) or empty to clear.
pub(crate) fn set_inline_table_inline_comment(
    it: &mut toml_edit::InlineTable,
    key: &str,
    inline: &str,
) {
    let next_key: Option<String> = {
        let mut iter = it.iter().skip_while(|(k, _)| *k != key);
        if iter.next().is_none() {
            return;
        }
        iter.next().map(|(k, _)| k.to_owned())
    };
    let raw = it_read_slot(it, next_key.as_deref()).to_owned();
    let current = PrefixParts::split(&raw).inline;
    if current == inline {
        return;
    }
    it_write_slot(
        it,
        next_key.as_deref(),
        &PrefixParts::with_inline(&raw, inline),
    );
}

/// Extract (and clear) an inline comment from a value's decor suffix.
///
/// When [`ItemProxy::clone_item`] clones an array element it copies the
/// slot-based inline comment into the value's decor suffix.  Array mutation
/// functions call this to retrieve that comment before pushing/replacing
/// the value, then feed it into the target array's slot system.
/// Returns the raw suffix (e.g. `" # note"`) or empty string if none,
/// matching the format used by [`CommentPreservation::save_inline_comments`].
pub(crate) fn take_value_inline_comment(value: &mut ValueRs) -> String {
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

// ---------------------------------------------------------------------------
// Array element block comments
// ---------------------------------------------------------------------------

/// Get the block comment from an element's decor prefix.
pub(crate) fn get_element_block_comment(decor: &toml_edit::Decor) -> Option<String> {
    let raw = decor.prefix()?.as_str()?;
    let parts = PrefixParts::split(raw);
    if parts.block.is_empty() {
        return None;
    }
    extract_block_comment(&parts.block)
}

/// Set the block comment in an element's decor prefix, preserving any inline
/// comment on the previous element and the indentation.
pub(crate) fn set_element_block_comment(
    decor: &mut toml_edit::Decor,
    comment: Option<&str>,
) -> PyResult<()> {
    let raw = decor.prefix().and_then(|r| r.as_str()).unwrap_or_default();
    let mut parts = PrefixParts::split(raw);
    parts.block = build_block_or_default(comment, &parts.indent, "")?;
    let new_prefix = parts.join();
    decor.set_prefix(new_prefix);
    Ok(())
}

// ---------------------------------------------------------------------------
// Inline-table block comment helpers
// ---------------------------------------------------------------------------

/// Whether `key` is the first key in the inline table's iteration order.
fn is_first_inline_table_key(it: &toml_edit::InlineTable, key: &str) -> bool {
    it.iter().next().is_some_and(|(k, _)| k == key)
}

/// Extract the block comment from an inline table key's prefix.
fn get_inline_table_block_comment(it: &toml_edit::InlineTable, key: &str) -> Option<String> {
    let raw = it.key(key)?.leaf_decor().prefix()?.as_str()?;
    if is_first_inline_table_key(it, key) {
        // The first key's prefix is `\n<comment>\n<indent>` (set inserts the
        // leading `\n` so the comment starts on its own line after `{`).
        // Strip the framing before extracting.
        extract_block_comment(raw.strip_prefix('\n').unwrap_or(raw).trim_end())
    } else {
        // Non-first key: the prefix mixes the previous key's inline comment
        // with this key's block comment; PrefixParts separates them.
        let parts = PrefixParts::split(raw);
        if parts.block.is_empty() {
            None
        } else {
            extract_block_comment(&parts.block)
        }
    }
}

/// Derive the canonical indent for an inline table.
///
/// Scans existing key prefixes and returns the first non-empty indent it can
/// recover. Compact inline tables often store this as a single leading space.
/// Falls back to `" "` when no key carries explicit indentation yet.
fn canonical_inline_table_indent(it: &toml_edit::InlineTable) -> String {
    for (key, _) in it.iter() {
        let indent = it
            .key(key)
            .and_then(|k| k.leaf_decor().prefix()?.as_str())
            .map(PrefixParts::split)
            .map(|parts| parts.indent)
            .unwrap_or_default();
        if !indent.is_empty() {
            return indent;
        }
    }
    " ".to_owned()
}

/// Set the block comment on an inline table key's prefix.
fn set_inline_table_block_comment(
    it: &mut toml_edit::InlineTable,
    key: &str,
    comment: Option<&str>,
) -> PyResult<()> {
    let is_first = is_first_inline_table_key(it, key);
    let canonical = comment.map(|_| canonical_inline_table_indent(it));
    let mut km = it
        .key_mut(key)
        .ok_or_else(|| PyKeyError::new_err(key.to_owned()))?;

    let raw = km
        .leaf_decor()
        .prefix()
        .and_then(|r| r.as_str())
        .unwrap_or_default()
        .to_owned();
    let mut parts = PrefixParts::split(&raw);

    // First key always uses the canonical indent. Newly inserted non-first keys
    // may have an empty prefix, so seed them from the table style as well.
    if let Some(ci) = canonical.filter(|_| is_first || parts.indent.is_empty()) {
        parts.indent = ci;
    }
    parts.block = build_block_or_default(comment, &parts.indent, "")?;

    let new_prefix = if is_first && parts.block.is_empty() {
        // Clearing the first key: just the indent.  Can't use parts.join() here
        // because it always inserts `\n` after `parts.inline`, which would add
        // a spurious blank line after `{`.
        parts.indent
    } else {
        parts.join()
    };
    km.leaf_decor_mut().set_prefix(new_prefix);
    Ok(())
}
