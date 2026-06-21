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
    trimmed.starts_with('#').then(|| trimmed.to_owned())
}

/// Extract block comment lines from a raw string.
/// Returns `#`-prefixed lines (trimmed of indentation) and empty lines for blank lines.
/// Trailing blank lines are dropped.  Returns `None` if there are no comment lines.
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
    while result.last().is_some_and(|s| s.is_empty()) {
        result.pop();
    }
    (!result.is_empty()).then(|| result.join("\n"))
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

/// Build a block-comment string from `comment`, or empty when `None`.
fn build_block_opt(comment: Option<&str>, indent: &str) -> PyResult<String> {
    Ok(comment
        .map(|t| build_block_comment(t, indent))
        .transpose()?
        .unwrap_or_default())
}

/// Decomposed parts of a table or key decor prefix.
///
/// A table/key prefix is `<leading><...block...><indent>` where:
///   - `leading` is the run of `\n` characters at the start of the prefix
///     (encoding blank-line separation from the previous entry)
///   - `indent`  is whatever follows the last `\n` (the indent before the
///     entry).
///
/// Any block comment between the two is owned by the caller, who replaces
/// it via `with_block`.
struct TablePrefix<'a> {
    leading: &'a str,
    indent: &'a str,
}

impl<'a> TablePrefix<'a> {
    fn split(prefix: &'a str) -> Self {
        let leading_end = prefix.bytes().take_while(|&b| b == b'\n').count();
        let leading = &prefix[..leading_end];
        let indent = prefix
            .rsplit_once('\n')
            .map_or(&prefix[leading_end..], |(_, after)| after);
        Self { leading, indent }
    }
}

impl TablePrefix<'_> {
    /// Build a new prefix that swaps the block comment in `existing` for
    /// `comment`, preserving the leading blank-line spacing and the
    /// trailing indent.
    fn with_block(existing: &str, comment: Option<&str>) -> PyResult<String> {
        let parts = TablePrefix::split(existing);
        let body = build_block_opt(comment, parts.indent)?;
        Ok(format!("{}{body}{}", parts.leading, parts.indent))
    }
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
    // An inline comment is on the value's own line — the first line of the
    // suffix.  Any later comment lines are block comments for what follows.
    value_suffix_inline(decor.suffix()?.as_str()?).map(str::to_owned)
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
            let decor = table
                .get_mut(key)
                .and_then(crate::dict_ops::header_decor_mut);
            if let Some(d) = decor {
                let existing = d.prefix().and_then(|r| r.as_str()).unwrap_or("").to_owned();
                let prefix = TablePrefix::with_block(&existing, comment)?;
                d.set_prefix(prefix);
                return Ok(());
            }
            let Some(mut km) = table.key_mut(key) else {
                return Err(PyKeyError::new_err(key.to_owned()));
            };
            let existing = km
                .leaf_decor()
                .prefix()
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_owned();
            let prefix = TablePrefix::with_block(&existing, comment)?;
            km.leaf_decor_mut().set_prefix(prefix);
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
// The comment always sits on the same line as the value, immediately before
// the line break that follows the element — so it lives wherever that break
// lives:
//   - the element's own decor **suffix**, when the break is there (the comma
//     starts the next line, "leading-comma" style, or it is the last element
//     before the closing `]`/`}`); or
//   - the **next** element's decor prefix (or the container's trailing string
//     for the last element), when the break sits after the comma
//     ("trailing-comma" style).
//
// `array_read_slot` / `it_read_slot` pick the right location; everything else
// (`SlotPrefix`, get/set, save/restore) is written once against the slot.
//
// A slot string has the form `{inline}\n{block}{indent}` where:
//   - `inline` is ` # text` (the element's inline comment) or empty
//   - `block`  is zero or more `{indent}# text\n` lines
//   - `indent` is the whitespace before the value
//
// The `CommentPreservation` trait abstracts over arrays vs inline tables so
// that save/restore operations are written once.

/// Decomposed parts of an element's slot string.
///
/// A slot is `<head><block><indent>` where:
///   - `head`   is empty (single-line layout) or `<inline-text>\n`
///     (the structural newline lives with the inline portion)
///   - `block`  is zero or more block comment lines, each terminated by `\n`
///   - `indent` is the trailing whitespace before the value
///
/// `split` and `join` are exact inverses for any input.
struct SlotPrefix {
    head: String,
    block: String,
    indent: String,
}

impl SlotPrefix {
    /// Decompose a raw slot string.
    fn split(raw: &str) -> Self {
        let Some((first_line, rest)) = raw.split_once('\n') else {
            return Self {
                head: String::new(),
                block: String::new(),
                indent: raw.to_owned(),
            };
        };
        let (block, indent) = match rest.rsplit_once('\n') {
            Some((block, indent)) => (format!("{block}\n"), indent.to_owned()),
            None => (String::new(), rest.to_owned()),
        };
        Self {
            head: format!("{first_line}\n"),
            block,
            indent,
        }
    }

    /// Reconstruct the raw slot string.
    fn join(&self) -> String {
        format!("{}{}{}", self.head, self.block, self.indent)
    }

    /// The inline-comment text (without the structural newline), or empty
    /// when the slot has no newline at all.
    fn inline(&self) -> &str {
        self.head.strip_suffix('\n').unwrap_or("")
    }

    /// Replace the block-comment portion.  A non-empty `block` ensures the
    /// structural newline is present (upgrading single-line layout); an
    /// empty `block` preserves the existing layout.
    fn set_block(&mut self, block: String) {
        if !block.is_empty() && self.head.is_empty() {
            self.head = String::from("\n");
        }
        self.block = block;
    }

    /// Return `raw` with its inline-comment portion replaced by `new_inline`.
    /// A non-empty `new_inline` ensures the structural newline is present
    /// (upgrading single-line layout); an empty `new_inline` preserves the
    /// existing layout.
    fn with_inline(raw: &str, new_inline: &str) -> String {
        let mut parts = Self::split(raw);
        if !new_inline.is_empty() || !parts.head.is_empty() {
            parts.head = format!("{new_inline}\n");
        }
        parts.join()
    }

    /// Return `raw` with its block-comment portion replaced by `comment`,
    /// preserving any inline comment on the previous element and the indent.
    fn with_block(raw: &str, comment: Option<&str>) -> PyResult<String> {
        let mut parts = Self::split(raw);
        parts.set_block(build_block_opt(comment, &parts.indent)?);
        Ok(parts.join())
    }
}

// -- Suffix break / inline-comment helpers --
//
// When the line break following an element lives in the element's own decor
// suffix (leading-comma layout, or the last element before `]`/`}`), the
// suffix has the form `{inline}\n{indent}`: an optional ` # comment` on the
// first line, then the structural whitespace positioning the comma or bracket.

/// The inline comment carried on the first line of a value's decor suffix,
/// trimmed (e.g. `# note`), or `None` if the first line has no comment.
fn value_suffix_inline(suffix: &str) -> Option<&str> {
    let first_line = suffix.split('\n').next().unwrap_or(suffix);
    let trimmed = first_line.trim();
    trimmed.starts_with('#').then_some(trimmed)
}

/// If a value's decor suffix carries a before-bracket inline comment, return
/// it in slot form (` # note`, ready to store after a separator); else `None`.
pub(crate) fn suffix_inline_as_slot(suffix: &str) -> Option<String> {
    value_suffix_inline(suffix).map(|c| format!(" {c}"))
}

/// Build a value suffix carrying `suffix`'s inline comment (if any) followed by
/// the structural whitespace `structural` (a separator or bracket layout).
pub(crate) fn suffix_with_structural(suffix: &str, structural: &str) -> String {
    let inline = suffix_inline_as_slot(suffix).unwrap_or_default();
    format!("{inline}{structural}")
}

/// The part of a value suffix from the first newline onward (the structural
/// whitespace that positions the following comma or bracket), or empty.
pub(crate) fn value_suffix_tail(suffix: &str) -> &str {
    suffix.find('\n').map_or("", |i| &suffix[i..])
}

/// The structural whitespace of a value suffix, with any leading before-bracket
/// inline comment removed.  When the suffix carries no such comment the whole
/// suffix is structural.
pub(crate) fn value_suffix_structural(suffix: &str) -> &str {
    if value_suffix_inline(suffix).is_some() {
        value_suffix_tail(suffix)
    } else {
        suffix
    }
}

/// A value's decor prefix / suffix as a `&str`, or `None` when unset.
pub(crate) fn value_prefix(v: &ValueRs) -> Option<&str> {
    v.decor().prefix().and_then(|r| r.as_str())
}
pub(crate) fn value_suffix(v: &ValueRs) -> Option<&str> {
    v.decor().suffix().and_then(|r| r.as_str())
}

/// Whether `suffix` carries the line break that follows its element — and thus
/// the element's inline comment.  This is the case for leading-comma elements
/// (the comma starts the next line) and for the last element before a closing
/// `]`/`}`; it is false for trailing-comma elements, whose break sits after the
/// comma in the next element's prefix.
pub(crate) fn suffix_holds_break(suffix: &str) -> bool {
    suffix.contains('\n') || value_suffix_inline(suffix).is_some()
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

/// Read the slot that holds array element `idx`'s inline comment: the line
/// break following the element carries the comment, so the slot is the
/// element's own suffix when that suffix holds the break (leading-comma layout,
/// or the last element before `]`), otherwise the next element's prefix (or the
/// array trailing for the last element).
fn array_read_slot(arr: &toml_edit::Array, idx: usize) -> &str {
    let own = arr.get(idx).and_then(value_suffix).unwrap_or_default();
    if suffix_holds_break(own) {
        own
    } else if idx + 1 < arr.len() {
        arr.get(idx + 1).and_then(value_prefix).unwrap_or_default()
    } else {
        arr.trailing().as_str().unwrap_or_default()
    }
}

/// Write the slot for array element `idx` (see [`array_read_slot`]).
fn array_write_slot(arr: &mut toml_edit::Array, idx: usize, raw: &str) {
    let in_suffix = arr
        .get(idx)
        .and_then(value_suffix)
        .is_some_and(suffix_holds_break);
    if in_suffix {
        arr.get_mut(idx).unwrap().decor_mut().set_suffix(raw);
    } else if idx + 1 < arr.len() {
        arr.get_mut(idx + 1).unwrap().decor_mut().set_prefix(raw);
    } else {
        arr.set_trailing(raw);
    }
}

impl CommentPreservation for toml_edit::Array {
    fn save_inline_comments(&self) -> Vec<String> {
        (0..self.len())
            .map(|i| {
                SlotPrefix::split(array_read_slot(self, i))
                    .inline()
                    .to_owned()
            })
            .collect()
    }

    fn restore_inline_comments(&mut self, comments: &[String]) {
        debug_assert_eq!(comments.len(), self.len());
        for (i, inline) in comments.iter().enumerate() {
            let raw = array_read_slot(self, i);
            if SlotPrefix::split(raw).inline() != inline {
                let new_raw = SlotPrefix::with_inline(raw, inline);
                array_write_slot(self, i, &new_raw);
            }
        }
    }
}

// -- InlineTable helpers --

/// Read the slot that holds inline-table entry `key`'s inline comment: the
/// value's own suffix when it carries the break (leading-comma layout, or the
/// last entry before `}`), otherwise the next key's leaf-decor prefix (or the
/// table trailing for the last entry).
fn it_read_slot<'a>(it: &'a toml_edit::InlineTable, key: &str, next_key: Option<&str>) -> &'a str {
    let own = it.get(key).and_then(value_suffix).unwrap_or_default();
    if suffix_holds_break(own) {
        own
    } else {
        match next_key {
            Some(k) => it
                .key(k)
                .and_then(|k| k.leaf_decor().prefix().and_then(|r| r.as_str()))
                .unwrap_or_default(),
            None => it.trailing().as_str().unwrap_or_default(),
        }
    }
}

/// Write the slot for inline-table entry `key` (see [`it_read_slot`]).
fn it_write_slot(it: &mut toml_edit::InlineTable, key: &str, next_key: Option<&str>, raw: &str) {
    let in_suffix = it
        .get(key)
        .and_then(value_suffix)
        .is_some_and(suffix_holds_break);
    if in_suffix {
        it.get_mut(key).unwrap().decor_mut().set_suffix(raw);
    } else {
        match next_key {
            Some(k) => it.key_mut(k).unwrap().leaf_decor_mut().set_prefix(raw),
            None => it.set_trailing(raw),
        }
    }
}

impl CommentPreservation for toml_edit::InlineTable {
    fn save_inline_comments(&self) -> Vec<String> {
        let keys: Vec<&str> = self.iter().map(|(k, _)| k).collect();
        (0..keys.len())
            .map(|i| {
                SlotPrefix::split(it_read_slot(self, keys[i], keys.get(i + 1).copied()))
                    .inline()
                    .to_owned()
            })
            .collect()
    }

    fn restore_inline_comments(&mut self, comments: &[String]) {
        debug_assert_eq!(comments.len(), self.len());
        let keys: Vec<String> = self.iter().map(|(k, _)| k.to_owned()).collect();
        for (i, inline) in comments.iter().enumerate() {
            let next_key = keys.get(i + 1).map(String::as_str);
            let raw = it_read_slot(self, &keys[i], next_key);
            if SlotPrefix::split(raw).inline() != inline {
                let new_raw = SlotPrefix::with_inline(raw, inline);
                it_write_slot(self, &keys[i], next_key, &new_raw);
            }
        }
    }
}

// -- Single-element comment access --

/// Get the inline comment for array element `idx` from a parent `ItemRs`.
pub(crate) fn get_array_inline_comment(parent: &ItemRs, idx: usize) -> Option<String> {
    let array = parent.as_value()?.as_array()?;
    extract_inline_comment(SlotPrefix::split(array_read_slot(array, idx)).inline())
}

/// Set the inline comment for array element `idx`.  `inline` is a
/// pre-validated raw suffix (e.g. `" # note"`) or empty to clear.
pub(crate) fn set_array_inline_comment(array: &mut toml_edit::Array, idx: usize, inline: &str) {
    let raw = array_read_slot(array, idx).to_owned();
    if SlotPrefix::split(&raw).inline() == inline {
        return;
    }
    array_write_slot(array, idx, &SlotPrefix::with_inline(&raw, inline));
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
    extract_inline_comment(SlotPrefix::split(it_read_slot(it, key, next_key)).inline())
}

/// Set the inline comment for an inline-table entry by key name.
/// `inline` is a pre-validated raw suffix (e.g. `" # note"`) or empty to clear.
pub(crate) fn set_inline_table_inline_comment(
    it: &mut toml_edit::InlineTable,
    key: &str,
    inline: &str,
) {
    // The caller navigated to `key`, so it exists; `next_key` is the entry
    // after it (or `None` when it is the last entry).
    let next_key: Option<String> = it
        .iter()
        .skip_while(|(k, _)| *k != key)
        .nth(1)
        .map(|(k, _)| k.to_owned());
    let raw = it_read_slot(it, key, next_key.as_deref()).to_owned();
    if SlotPrefix::split(&raw).inline() == inline {
        return;
    }
    it_write_slot(
        it,
        key,
        next_key.as_deref(),
        &SlotPrefix::with_inline(&raw, inline),
    );
}

/// Extract (and clear) an inline comment from a value's decor suffix.
///
/// When [`ItemProxy::clone_item`] clones an array element it copies the
/// element's inline comment into the cloned value's decor suffix.  Array
/// mutation functions call this to retrieve that comment before pushing or
/// replacing the value, then feed it into the target array's slot system.
/// Returns the comment in slot form (e.g. `" # note"`) or an empty string if
/// none, matching the format used by
/// [`CommentPreservation::save_inline_comments`].  Any structural whitespace
/// following the comment (the comma-on-next-line tail) is discarded along with
/// the comment.
pub(crate) fn take_value_inline_comment(value: &mut ValueRs) -> String {
    let suffix = value
        .decor()
        .suffix()
        .and_then(|r| r.as_str())
        .unwrap_or_default();
    if let Some(c) = value_suffix_inline(suffix) {
        let result = format!(" {c}");
        value.decor_mut().set_suffix("");
        result
    } else {
        String::new()
    }
}

/// Remove a before-bracket inline comment from a value's own decor suffix,
/// keeping the structural whitespace that follows it.  No-op when the suffix
/// carries no such comment.
pub(crate) fn strip_value_suffix_inline(value: &mut ValueRs) {
    let suffix = value
        .decor()
        .suffix()
        .and_then(|r| r.as_str())
        .unwrap_or_default();
    let structural = value_suffix_structural(suffix);
    if structural != suffix {
        let structural = structural.to_owned();
        value.decor_mut().set_suffix(structural);
    }
}

// ---------------------------------------------------------------------------
// Array element block comments
// ---------------------------------------------------------------------------

/// Get the block comment from an element's decor prefix.
pub(crate) fn get_element_block_comment(decor: &toml_edit::Decor) -> Option<String> {
    let raw = decor.prefix()?.as_str()?;
    let parts = SlotPrefix::split(raw);
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
    decor.set_prefix(SlotPrefix::with_block(raw, comment)?);
    Ok(())
}

/// Get the block comment above array element `idx`.
///
/// A block comment occupies the inter-value region before `idx`: the previous
/// element's suffix (before the comma) followed by this element's own prefix
/// (after the comma).  Concatenating them and letting [`SlotPrefix`] separate
/// the previous element's inline head from the block lines yields the comment
/// for every layout — first element, trailing-comma, and leading-comma with the
/// comment on either side of the comma.
pub(crate) fn get_array_element_block_comment(
    arr: &toml_edit::Array,
    idx: usize,
) -> Option<String> {
    let prev_suffix = idx
        .checked_sub(1)
        .and_then(|p| arr.get(p))
        .and_then(value_suffix)
        .unwrap_or_default();
    let own_prefix = arr.get(idx).and_then(value_prefix).unwrap_or_default();
    let block = SlotPrefix::split(&format!("{prev_suffix}{own_prefix}")).block;
    (!block.is_empty())
        .then(|| extract_block_comment(&block))
        .flatten()
}

/// The block-comment lines of a prefix whose first line is itself a block
/// comment (no leading inline head) — as in a leading-comma element's own
/// prefix.  Returned with each line's trailing newline preserved.
fn prefix_only_block(prefix: &str) -> String {
    SlotPrefix::split(&format!("\n{prefix}")).block
}

/// Set or clear the block comment above array element `idx`
/// (see [`get_array_element_block_comment`]).
pub(crate) fn set_array_element_block_comment(
    arr: &mut toml_edit::Array,
    idx: usize,
    comment: Option<&str>,
) -> PyResult<()> {
    if idx == 0 {
        // The caller navigated to this element, so element 0 exists.
        let decor = arr.get_mut(0).expect("element exists").decor_mut();
        return set_element_block_comment(decor, comment);
    }
    // Write the block into the slot (the canonical location, before the comma).
    let raw = array_read_slot(arr, idx - 1).to_owned();
    array_write_slot(arr, idx - 1, &SlotPrefix::with_block(&raw, comment)?);
    // In leading-comma layout, drop any stale block sitting after the comma
    // (the element's own prefix) so the comment is not duplicated.
    if arr
        .get(idx - 1)
        .and_then(value_suffix)
        .is_some_and(suffix_holds_break)
    {
        let own = arr.get(idx).and_then(value_prefix).unwrap_or_default();
        if !prefix_only_block(own).is_empty() {
            arr.get_mut(idx).unwrap().decor_mut().set_prefix("");
        }
    }
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
        // with this key's block comment; SlotPrefix separates them.
        extract_block_comment(&SlotPrefix::split(raw).block)
    }
}

/// Derive the canonical indent for an inline table.
///
/// Scans existing key prefixes and returns the first non-empty indent it can
/// recover. Compact inline tables often store this as a single leading space.
/// Falls back to `" "` when no key carries explicit indentation yet.
fn canonical_inline_table_indent(it: &toml_edit::InlineTable) -> String {
    for (key, _) in it {
        let indent = it
            .key(key)
            .and_then(|k| k.leaf_decor().prefix()?.as_str())
            .map(SlotPrefix::split)
            .map(|parts| parts.indent)
            .unwrap_or_default();
        if !indent.is_empty() {
            return indent;
        }
    }
    " ".to_owned()
}

/// True if `it` uses multi-line layout (entries separated by newlines), as
/// opposed to a compact single-line table.  `exclude` is ignored when scanning
/// (the freshly-inserted key, which has no newline yet).
fn is_multiline_inline_table(it: &toml_edit::InlineTable, exclude: &str) -> bool {
    if it.trailing().as_str().is_some_and(|s| s.contains('\n')) {
        return true;
    }
    it.iter().any(|(k, _)| {
        k != exclude
            && it
                .key(k)
                .and_then(|key| key.leaf_decor().prefix()?.as_str())
                .is_some_and(|s| s.contains('\n'))
    })
}

/// Align a freshly-inserted key in a multi-line inline table so it sits on its
/// own indented line, matching its siblings.  No-op for compact (single-line)
/// inline tables, whose new keys correctly stay on the same line.
pub(crate) fn align_inserted_inline_key(it: &mut toml_edit::InlineTable, key: &str) {
    if !is_multiline_inline_table(it, key) {
        return;
    }
    let indent = canonical_inline_table_indent(it);
    // The caller inserts `key` immediately before calling us, so the lookup
    // cannot fail; toml_edit only exposes the mutable handle via `Option`.
    let mut km = it
        .key_mut(key)
        .expect("key was just inserted by the caller");
    let raw = km
        .leaf_decor()
        .prefix()
        .and_then(|r| r.as_str())
        .unwrap_or_default()
        .to_owned();
    let mut parts = SlotPrefix::split(&raw);
    if parts.head.is_empty() {
        parts.head = "\n".to_owned();
    }
    parts.indent = indent;
    km.leaf_decor_mut().set_prefix(parts.join());
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
    let mut parts = SlotPrefix::split(&raw);

    // First key always uses the canonical indent. Newly inserted non-first keys
    // may have an empty prefix, so seed them from the table style as well.
    if let Some(ci) = canonical.filter(|_| is_first || parts.indent.is_empty()) {
        parts.indent = ci;
    }
    parts.set_block(build_block_opt(comment, &parts.indent)?);
    km.leaf_decor_mut().set_prefix(parts.join());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SlotPrefix::split` followed by `SlotPrefix::join` must be the
    /// identity for any input — no spurious or lost newlines.
    #[test]
    fn slot_prefix_split_join_round_trip() {
        for raw in [
            "",
            "  ",
            "\n",
            "\n  ",
            "\n\n  ",
            " # foo\n  # bar\n  ",
            "\n\n# block\n  ",
        ] {
            assert_eq!(
                SlotPrefix::split(raw).join(),
                raw,
                "round-trip failed for {raw:?}"
            );
        }
    }
}
