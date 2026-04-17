use pyo3::exceptions::{PyIndexError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PySlice;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::comments::CommentPreservation;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{Affected, into_value};

// ---------------------------------------------------------------------------
// Array-like enum — constrains list operations to valid item types
// ---------------------------------------------------------------------------

/// Mutable reference to an array-like TOML item (array or array-of-tables).
pub(crate) enum ArrayLikeMut<'a> {
    Array(&'a mut toml_edit::Array),
    Aot(&'a mut toml_edit::ArrayOfTables),
}

impl ArrayLikeMut<'_> {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Array(arr) => arr.len(),
            Self::Aot(aot) => aot.len(),
        }
    }
}

/// Shared reference to an array-like TOML item.
#[derive(Clone, Copy)]
pub(crate) enum ArrayLikeRef<'a> {
    Array(&'a toml_edit::Array),
    Aot(&'a toml_edit::ArrayOfTables),
}

impl ArrayLikeRef<'_> {
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Array(arr) => arr.len(),
            Self::Aot(aot) => aot.len(),
        }
    }
}

/// Owned array-like item — the owned counterpart to `ArrayLikeRef`.
///
/// Used by proxy fast paths that need to hold the source data across a
/// destination write lock (so that per-element handling on kind mismatch
/// doesn't require re-entering Python iteration).
pub(crate) enum ArrayLikeOwned {
    Array(toml_edit::Array),
    Aot(toml_edit::ArrayOfTables),
}

impl ArrayLikeOwned {
    /// Clone `item` into an `ArrayLikeOwned` if it's array-like.
    pub(crate) fn from_item(item: &ItemRs) -> Option<Self> {
        match item {
            ItemRs::Value(ValueRs::Array(arr)) => Some(Self::Array(arr.clone())),
            ItemRs::ArrayOfTables(aot) => Some(Self::Aot(aot.clone())),
            _ => None,
        }
    }

    pub(crate) fn as_ref(&self) -> ArrayLikeRef<'_> {
        match self {
            Self::Array(arr) => ArrayLikeRef::Array(arr),
            Self::Aot(aot) => ArrayLikeRef::Aot(aot),
        }
    }

    /// Decompose into elements as `Item`s.
    pub(crate) fn into_items(self) -> Vec<Item> {
        match self {
            Self::Array(arr) => arr.into_iter().map(|v| Item(ItemRs::Value(v))).collect(),
            Self::Aot(aot) => aot.into_iter().map(|t| Item(ItemRs::Table(t))).collect(),
        }
    }
}

/// Extract a mutable array-like reference, or return a `TypeError`.
pub(crate) fn as_array_like_mut<'a>(item: &'a mut ItemRs, op: &str) -> PyResult<ArrayLikeMut<'a>> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => Ok(ArrayLikeMut::Array(arr)),
        ItemRs::ArrayOfTables(aot) => Ok(ArrayLikeMut::Aot(aot)),
        _ => Err(PyTypeError::new_err(format!(
            "'{}' does not support {op}",
            item.type_name()
        ))),
    }
}

/// Extract a shared array-like reference, or return a `TypeError`.
pub(crate) fn as_array_like<'a>(item: &'a ItemRs, op: &str) -> PyResult<ArrayLikeRef<'a>> {
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => Ok(ArrayLikeRef::Array(arr)),
        ItemRs::ArrayOfTables(aot) => Ok(ArrayLikeRef::Aot(aot)),
        _ => Err(PyTypeError::new_err(format!(
            "'{}' does not support {op}",
            item.type_name()
        ))),
    }
}

/// Check if `obj` is a plain `list` or a `ListItem` proxy.
pub(crate) fn is_list_like(obj: &Bound<'_, PyAny>, py: Python<'_>) -> bool {
    obj.is_instance_of::<pyo3::types::PyList>()
        || obj
            .is_instance(&py.get_type::<crate::list_proxy::ListProxy>())
            .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Extract just the `\n{indent}` portion of a multiline prefix string,
/// stripping any block comments that precede the indentation.
/// Returns `None` when the prefix contains no newline (single-line array).
fn indent_only(raw: &str) -> Option<String> {
    raw.rsplit_once('\n')
        .map(|(_, indent)| format!("\n{indent}"))
}

/// Detect whether an array uses multiline formatting and return the element
/// indentation prefix if so (e.g. `"\n    "`).  Returns `None` for single-line
/// arrays.
///
/// Only the indentation is returned — any block comments on the first element
/// are excluded so that newly inserted/appended values don't inherit them.
fn multiline_prefix(arr: &toml_edit::Array) -> Option<String> {
    let first = arr.get(0)?;
    let raw = first.decor().prefix()?.as_str()?;
    indent_only(raw)
}

/// For a single-line array, return the prefix a newly inserted element
/// should have, matching the array's inter-element separator.  Returns
/// `None` for multiline or empty arrays.
fn single_line_separator(arr: &toml_edit::Array) -> Option<String> {
    if multiline_prefix(arr).is_some() {
        return None;
    }
    inter_element_prefix(arr)
}

/// The prefix of an existing non-first element — the whitespace that
/// follows a comma in a single-line array.  Falls back to `" "` when the
/// array has only one element; `None` when the array is empty.
///
/// Only meaningful for single-line arrays; on a multiline array element 1's
/// prefix includes the line break and possibly a block comment.
fn inter_element_prefix(arr: &toml_edit::Array) -> Option<String> {
    if arr.is_empty() {
        return None;
    }
    let sep = arr
        .get(1)
        .and_then(|e| e.decor().prefix())
        .and_then(|r| r.as_str())
        .unwrap_or(" ");
    Some(sep.to_owned())
}

/// Apply decor to a newly created value, matching the array's existing style.
fn apply_element_decor(arr: &toml_edit::Array, v: &mut ValueRs) {
    if let Some(prefix) = multiline_prefix(arr) {
        let decor = v.decor_mut();
        decor.set_prefix(prefix);
        decor.set_suffix("");
    } else if let Some(sep) = inter_element_prefix(arr) {
        v.decor_mut().set_prefix(sep);
    }
}

/// Read a value's prefix string.
fn value_prefix(v: &ValueRs) -> Option<&str> {
    v.decor().prefix().and_then(|r| r.as_str())
}

/// Read a value's suffix string.
fn value_suffix(v: &ValueRs) -> Option<&str> {
    v.decor().suffix().and_then(|r| r.as_str())
}

/// Strip the last element's trailing suffix (whitespace before `]`) and return
/// it.  Returns `None` if the array is empty or the suffix is empty.
fn strip_last_suffix(arr: &mut toml_edit::Array) -> Option<String> {
    let last = arr.get_mut(arr.len().checked_sub(1)?)?;
    let suffix = value_suffix(last).filter(|s| !s.is_empty())?.to_owned();
    last.decor_mut().set_suffix("");
    Some(suffix)
}

/// Apply a saved trailing suffix to the current last element.
fn apply_last_suffix(arr: &mut toml_edit::Array, suffix: Option<String>) {
    if let Some(s) = suffix
        && let Some(last) = arr.get_mut(arr.len() - 1)
    {
        last.decor_mut().set_suffix(&s);
    }
}

/// Run `body` on `arr` with the last-element seam preserved across the
/// operation.  The inline comment attached to the current last element
/// (stored in the array's trailing slot) and the last element's suffix
/// are both saved beforehand and re-applied afterwards.
///
/// `body` receives the inline-comment vector and must push one entry per
/// element it appends so `restore_inline_comments` lands each comment in
/// the right slot.
fn with_preserved_seam(
    arr: &mut toml_edit::Array,
    body: impl FnOnce(&mut toml_edit::Array, &mut Vec<String>),
) {
    let saved_suffix = strip_last_suffix(arr);
    let mut inlines = arr.save_inline_comments();
    body(arr, &mut inlines);
    arr.restore_inline_comments(&inlines);
    apply_last_suffix(arr, saved_suffix);
}

/// Save the first element's leading prefix (whitespace after `[`).
/// Returns `None` if the array is empty or the prefix is empty.
///
/// For multiline arrays, only the structural indentation (`\n` + indent)
/// is saved — block comments that belong to the first element are excluded
/// so they stay attached to that element after an insertion at position 0.
fn save_first_prefix(arr: &toml_edit::Array) -> Option<String> {
    let raw = arr
        .get(0)
        .and_then(|v| value_prefix(v))
        .filter(|s| !s.is_empty())?;
    indent_only(raw).or_else(|| Some(raw.to_owned()))
}

/// Apply a saved leading prefix to the current first element.
fn apply_first_prefix(arr: &mut toml_edit::Array, prefix: Option<String>) {
    if let Some(p) = prefix
        && let Some(first) = arr.get_mut(0)
    {
        first.decor_mut().set_prefix(&p);
    }
}

/// Save the decor prefix of an AoT entry at `index`.
///
/// Block comments and spacing separators live in each table's decor prefix.
/// Mutations that replace an entry must save this first, then stamp the
/// saved prefix onto the new table before inserting/replacing it.
fn save_aot_entry_prefix(aot: &toml_edit::ArrayOfTables, index: usize) -> String {
    aot.get(index)
        .and_then(|t| t.decor().prefix()?.as_str())
        .unwrap_or_default()
        .to_owned()
}

/// Clean up the first AoT entry's decor prefix after a removal.
///
/// When entry 0 is removed, the new first entry may have a leading `\n`
/// separator (left over from being a non-first entry).  Strip it.
fn fix_first_aot_prefix(aot: &mut toml_edit::ArrayOfTables) {
    let Some(first) = aot.get_mut(0) else {
        return;
    };
    if let Some(stripped) = first
        .decor()
        .prefix()
        .and_then(|r| r.as_str())
        .and_then(|s| s.strip_prefix('\n'))
    {
        let stripped = stripped.to_owned();
        first.decor_mut().set_prefix(stripped);
    }
}

/// After inserting a new table at position `pos`, ensure the affected element
/// has the correct blank-line spacing to match its neighbours.
///
/// We only *detect* whether the nearest non-first neighbour uses a leading
/// `\n` (blank-line separator) and then add or strip a leading `\n` on the
/// target.  The rest of the prefix (block comments, etc.) is never touched.
///
/// When inserting at position 0 the new element needs no prefix (it is now
/// first), but the *old* first element — now at position 1 — was re-pushed
/// with its original prefix and must be fixed instead.
fn fix_inserted_aot_spacing(aot: &mut toml_edit::ArrayOfTables, pos: usize) {
    // Detect whether the nearest non-first neighbour uses blank-line spacing.
    // Prefer the element *before* — elements after may also be newly pushed
    // and not yet corrected.
    //
    // A `None` prefix means "unset" — toml_edit will insert a default blank
    // line for non-first AoT entries, so we treat it as spaced.  Only an
    // explicitly set prefix that does NOT start with '\n' counts as compact.
    //
    // Inserting at the front: the element that needs fixing is the old first
    // element, now sitting at position 1.
    let target = if pos == 0 { 1 } else { pos };
    let spaced = [target - 1, target + 1]
        .into_iter()
        .filter(|&i| i > 0 && i < aot.len())
        .find_map(|i| {
            aot.get(i).map(|t| {
                let as_str = t.decor().prefix().and_then(|r| r.as_str());
                // None → default blank line (spaced)
                // Some(s) starting with '\n' → explicitly spaced
                // Some(s) not starting with '\n' → compact
                as_str.is_none() || as_str.is_some_and(|s| s.starts_with('\n'))
            })
        })
        .unwrap_or(true);

    // Read the target's current prefix.  Distinguish between `None` (unset —
    // toml_edit will insert a default blank line) and `Some("")` (explicitly
    // empty — no blank line).
    let Some(entry) = aot.get_mut(target) else {
        return;
    };
    let raw_prefix = entry
        .decor()
        .prefix()
        .and_then(|r| r.as_str())
        .map(str::to_owned);
    let current = raw_prefix.as_deref().unwrap_or("");
    let has_blank = current.starts_with('\n');

    if spaced && !has_blank {
        // Prepend a blank line, preserving any existing content (comments).
        entry.decor_mut().set_prefix(format!("\n{current}"));
    } else if !spaced && (raw_prefix.is_none() || current == "\n") {
        // Prefix is either unset (toml_edit would insert a default blank
        // line) or is a bare "\n".  Explicitly set to "" to suppress it.
        entry.decor_mut().set_prefix("");
    }
}

fn require_table(item: Item) -> PyResult<toml_edit::Table> {
    let mut table = match item.0 {
        ItemRs::Table(t) => t,
        ItemRs::Value(ValueRs::InlineTable(it)) => it.into_table(),
        other => {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "cannot append {} to array of tables (expected a table/dict)",
                other.type_name()
            )));
        }
    };
    // Clear any source position — when a table is cloned from another
    // document, its span would otherwise cause toml_edit to interleave the
    // rendering of AoT entries in span order rather than push order.
    table.set_position(None);
    Ok(table)
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
// Index / slice helpers
// ---------------------------------------------------------------------------

/// Resolve a Python index (possibly negative) against a known length.
pub(crate) fn resolve_index(index: i64, len: usize) -> PyResult<usize> {
    let resolved = if index < 0 { len as i64 + index } else { index };
    if resolved < 0 || resolved as usize >= len {
        Err(PyIndexError::new_err("index out of range"))
    } else {
        Ok(resolved as usize)
    }
}

/// Resolve an integer index against an array-like item.
pub(crate) fn require_array_index(item: &ItemRs, index: i64) -> PyResult<usize> {
    let target = as_array_like(item, "indexing")?;
    resolve_index(index, target.len())
}

/// Collect concrete indices from resolved slice parameters.
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

/// Delete elements at the given indices (sorted in reverse internally).
pub(crate) fn item_delitem_slice(target: ArrayLikeMut<'_>, indices: &[usize]) -> PyResult<()> {
    let mut sorted = indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted.reverse();

    match target {
        ArrayLikeMut::Array(arr) => {
            let mut ic = arr.save_inline_comments();
            let removing_first = sorted.last() == Some(&0);
            let removing_last = !sorted.is_empty() && sorted.first() == Some(&(arr.len() - 1));
            let decor = save_removal_decor(arr, removing_first, removing_last);
            for idx in sorted {
                arr.remove(idx);
                ic.remove(idx);
            }
            arr.restore_inline_comments(&ic);
            apply_removal_decor(arr, &decor);
            Ok(())
        }
        ArrayLikeMut::Aot(aot) => {
            let removing_first = sorted.last() == Some(&0);
            for idx in sorted {
                aot.remove(idx);
            }
            if removing_first {
                fix_first_aot_prefix(aot);
            }
            Ok(())
        }
    }
}

/// Assign to a slice of an array-like item.
pub(crate) fn item_setitem_slice(
    target: ArrayLikeMut<'_>,
    start: isize,
    stop: isize,
    step: isize,
    values: Vec<Item>,
) -> PyResult<()> {
    match target {
        ArrayLikeMut::Array(arr) => array_setitem_slice(arr, start, stop, step, values),
        ArrayLikeMut::Aot(aot) => aot_setitem_slice(aot, start, stop, step, values),
    }
}

fn array_setitem_slice(
    arr: &mut toml_edit::Array,
    start: isize,
    stop: isize,
    step: isize,
    values: Vec<Item>,
) -> PyResult<()> {
    // Validate all values up front so a bad element never leaves the array
    // in a partially-mutated state (mirrors the AoT path).
    let converted: Vec<ValueRs> = values
        .into_iter()
        .map(into_value)
        .collect::<PyResult<_>>()?;

    if step == 1 {
        // Contiguous slice: replacement can be a different length.
        let start_idx = start as usize;
        let stop_idx = stop as usize;

        let mut ic = arr.save_inline_comments();
        let removes_first = start_idx == 0 && stop_idx > 0;
        let removes_last = stop_idx == arr.len() && stop_idx > start_idx;
        let inserting = !converted.is_empty();
        let decor =
            save_removal_decor(arr, removes_first && !inserting, removes_last && !inserting);

        // Save boundary spacing (space after `[` / before `]`) before mutating.
        let first_prefix = (start_idx == 0 && inserting)
            .then(|| save_first_prefix(arr))
            .flatten();
        let last_suffix = (stop_idx == arr.len() && inserting)
            .then(|| strip_last_suffix(arr))
            .flatten();

        // Remove old elements from back to front.
        for i in (start_idx..stop_idx).rev() {
            arr.remove(i);
            ic.remove(i);
        }

        // Insert new elements at start position.
        for (offset, mut v) in converted.into_iter().enumerate() {
            let inline = comments::take_value_inline_comment(&mut v);
            let idx = start_idx + offset;
            if idx >= arr.len() {
                arr.push(v);
            } else {
                arr.insert(idx, v);
            }
            ic.insert(idx, inline);
        }

        // Restore boundary spacing.
        apply_first_prefix(arr, first_prefix);
        apply_last_suffix(arr, last_suffix);

        arr.restore_inline_comments(&ic);
        apply_removal_decor(arr, &decor);
    } else {
        // Extended slice: replacement must match the slice length.
        let indices = collect_slice_indices(start, stop, step);
        if indices.len() != converted.len() {
            return Err(PyValueError::new_err(format!(
                "attempt to assign sequence of size {} to extended slice of size {}",
                converted.len(),
                indices.len()
            )));
        }
        for (idx, mut v) in indices.into_iter().zip(converted) {
            let inline = comments::take_value_inline_comment(&mut v);
            arr.replace(idx, v);
            if !inline.is_empty() {
                comments::set_array_inline_comment(arr, idx, &inline);
            }
        }
    }
    Ok(())
}

fn aot_setitem_slice(
    aot: &mut toml_edit::ArrayOfTables,
    start: isize,
    stop: isize,
    step: isize,
    values: Vec<Item>,
) -> PyResult<()> {
    // Validate all values up front so a bad element never leaves the AoT
    // in a partially-mutated state.
    let mut tables: Vec<toml_edit::Table> = values
        .into_iter()
        .map(require_table)
        .collect::<PyResult<_>>()?;

    if step == 1 {
        let start_idx = start as usize;
        let stop_idx = stop as usize;
        let tables_count = tables.len();
        let removes_first = start_idx == 0 && stop_idx > 0;
        let saved = removes_first.then(|| save_aot_entry_prefix(aot, 0));
        for i in (start_idx..stop_idx).rev() {
            aot.remove(i);
        }
        if removes_first && tables_count == 0 {
            fix_first_aot_prefix(aot);
        }
        if let Some(prefix) = &saved
            && let Some(first) = tables.first_mut()
        {
            first.decor_mut().set_prefix(prefix);
        }
        for (offset, table) in tables.into_iter().enumerate() {
            aot.insert(start_idx + offset, table);
        }
        // fix_inserted_aot_spacing(aot, 0) targets index 1.  When that is
        // an old survivor (tables_count <= 1) rather than a newly inserted
        // entry, skip — its prefix is already correct, and the spacing
        // detector can't infer the style from index 0 alone.
        let spacing_start = if start_idx == 0 && tables_count <= 1 {
            1
        } else {
            start_idx
        };
        for i in spacing_start..start_idx + tables_count {
            fix_inserted_aot_spacing(aot, i);
        }
    } else {
        let indices = collect_slice_indices(start, stop, step);
        if indices.len() != tables.len() {
            return Err(PyValueError::new_err(format!(
                "attempt to assign sequence of size {} to extended slice of size {}",
                tables.len(),
                indices.len()
            )));
        }
        // Save all affected entries' prefixes, then stamp them onto the new
        // tables before replacing so the decor travels with the table.
        let saved: Vec<String> = indices
            .iter()
            .map(|&idx| save_aot_entry_prefix(aot, idx))
            .collect();
        for (table, prefix) in tables.iter_mut().zip(&saved) {
            table.decor_mut().set_prefix(prefix);
        }
        for (idx, table) in indices.into_iter().zip(tables) {
            aot.replace(idx, table);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Array boundary-element decoration repair
// ---------------------------------------------------------------------------

/// Decoration state captured before an array removal so that the opening and
/// closing brackets stay in their original positions.
struct RemovalDecor {
    pub(crate) first_prefix: Option<String>,
    pub(crate) last_suffix: Option<String>,
}

/// Snapshot the decorations that would be lost when the first and/or last
/// element of an array is removed.
///
/// `removing_first` / `removing_last` indicate whether the removal will
/// affect element 0 or element `len − 1`.  Returns `None` fields when the
/// corresponding boundary is unaffected or the array is too small to need
/// repair (single-element arrays becoming empty).
fn save_removal_decor(
    arr: &toml_edit::Array,
    removing_first: bool,
    removing_last: bool,
) -> RemovalDecor {
    let at_least_two = arr.len() >= 2;
    RemovalDecor {
        first_prefix: (removing_first && at_least_two).then(|| {
            value_prefix(arr.get(0).expect("at_least_two"))
                .unwrap_or_default()
                .to_owned()
        }),
        last_suffix: (removing_last && at_least_two).then(|| {
            value_suffix(arr.get(arr.len() - 1).expect("at_least_two"))
                .unwrap_or_default()
                .to_owned()
        }),
    }
}

/// Apply saved decoration fixes after a removal + `restore_inline_comments`.
fn apply_removal_decor(arr: &mut toml_edit::Array, decor: &RemovalDecor) {
    // --- First-element prefix ---
    // The new first element inherits a prefix meant to follow a comma (e.g.
    // `" "` in `[1, 2]` or `" # note\n    "` in multiline arrays).  Replace
    // it with the old first element's prefix so `[1, 2, 3]` becomes `[2, 3]`
    // instead of `[ 2, 3]`.
    if let Some(ref old_first_prefix) = decor.first_prefix
        && let Some(new_first) = arr.get_mut(0)
    {
        let cur = value_prefix(new_first).unwrap_or_default().to_owned();
        let fixed = if let Some((_inline, rest)) = cur.split_once('\n') {
            // Multiline: drop the removed element's inline-comment line,
            // keep block comments + indentation that belong to this element.
            format!("\n{rest}")
        } else {
            // Single-line: use the original first element's prefix.
            old_first_prefix.clone()
        };
        new_first.decor_mut().set_prefix(&fixed);
    }

    // --- Last-element suffix ---
    // The old last element's suffix (whitespace between the value and `]`) is
    // discarded by toml_edit when that element is removed.  Transfer it to the
    // array's trailing string so the closing bracket stays in place, e.g.
    // `[ 1, 2, 3 ]` becomes `[ 1, 2 ]` instead of `[ 1, 2]`.
    if let Some(ref old_last_suffix) = decor.last_suffix
        && !old_last_suffix.is_empty()
        && !arr.is_empty()
    {
        let trailing = arr.trailing().as_str().unwrap_or_default().to_owned();
        arr.set_trailing(format!("{trailing}{old_last_suffix}"));
    }
}

// ---------------------------------------------------------------------------
// List-like operations
// ---------------------------------------------------------------------------

pub(crate) fn item_append(target: ArrayLikeMut<'_>, value: Item) -> PyResult<()> {
    match target {
        ArrayLikeMut::Array(arr) => {
            let mut v = into_value(value)?;
            let inline = comments::take_value_inline_comment(&mut v);
            with_preserved_seam(arr, |arr, ic| {
                apply_element_decor(arr, &mut v);
                arr.push(v);
                ic.push(inline);
            });
            Ok(())
        }
        ArrayLikeMut::Aot(aot) => {
            let table = require_table(value)?;
            aot.push(table);
            fix_inserted_aot_spacing(aot, aot.len() - 1);
            Ok(())
        }
    }
}

/// Insert an element.  Returns `Some(Affected::Shift(..))` when the
/// insertion shifted existing indices, or `None` when appended at the end.
pub(crate) fn item_insert(
    target: ArrayLikeMut<'_>,
    index: i64,
    value: Item,
) -> PyResult<Option<Affected>> {
    match target {
        ArrayLikeMut::Array(arr) => {
            let resolved = clamp_index(index, arr.len());
            let at_end = resolved == arr.len();
            let at_start = resolved == 0 && !arr.is_empty();
            let saved_suffix = at_end.then(|| strip_last_suffix(arr)).flatten();
            let saved_prefix = at_start.then(|| save_first_prefix(arr)).flatten();
            let mut ic = arr.save_inline_comments();
            let mut v = into_value(value)?;
            let inline = comments::take_value_inline_comment(&mut v);
            apply_element_decor(arr, &mut v);
            arr.insert(resolved, v);
            ic.insert(resolved, inline);
            arr.restore_inline_comments(&ic);
            apply_last_suffix(arr, saved_suffix);
            apply_first_prefix(arr, saved_prefix);
            Ok((!at_end).then_some(Affected::Range {
                from: resolved,
                to: arr.len(),
            }))
        }
        ArrayLikeMut::Aot(aot) => {
            let resolved = clamp_index(index, aot.len());
            let at_end = resolved == aot.len();
            let table = require_table(value)?;
            aot.insert(resolved, table);
            fix_inserted_aot_spacing(aot, resolved);
            Ok((!at_end).then_some(Affected::Range {
                from: resolved,
                to: aot.len(),
            }))
        }
    }
}

/// Remove the element at `index`.  Returns the removed item and an
/// `Affected` descriptor for proxy invalidation.
pub(crate) fn item_remove_at(target: ArrayLikeMut<'_>, index: usize) -> PyResult<(Item, Affected)> {
    match target {
        ArrayLikeMut::Array(arr) => {
            if index >= arr.len() {
                return Err(PyIndexError::new_err("array index out of range"));
            }
            let affected = Affected::for_removal(index, arr.len());
            let mut ic = arr.save_inline_comments();
            let decor = save_removal_decor(arr, index == 0, index == arr.len() - 1);
            let removed = arr.remove(index);
            ic.remove(index);
            arr.restore_inline_comments(&ic);
            apply_removal_decor(arr, &decor);
            Ok((Item(ItemRs::Value(removed)), affected))
        }
        ArrayLikeMut::Aot(aot) => {
            if index >= aot.len() {
                return Err(PyIndexError::new_err("array index out of range"));
            }
            let affected = Affected::for_removal(index, aot.len());
            let removed = aot.remove(index);
            if index == 0 {
                fix_first_aot_prefix(aot);
            }
            Ok((Item(ItemRs::Table(removed)), affected))
        }
    }
}

pub(crate) fn item_extend(target: ArrayLikeMut<'_>, items: Vec<Item>) -> PyResult<()> {
    match target {
        ArrayLikeMut::Array(arr) => {
            // Validate all values up front.
            let converted: Vec<ValueRs> =
                items.into_iter().map(into_value).collect::<PyResult<_>>()?;
            with_preserved_seam(arr, |arr, ic| {
                for mut v in converted {
                    ic.push(comments::take_value_inline_comment(&mut v));
                    apply_element_decor(arr, &mut v);
                    arr.push(v);
                }
            });
            Ok(())
        }
        ArrayLikeMut::Aot(aot) => {
            // Validate all values up front.
            let tables: Vec<toml_edit::Table> = items
                .into_iter()
                .map(require_table)
                .collect::<PyResult<_>>()?;
            for table in tables {
                aot.push(table);
                fix_inserted_aot_spacing(aot, aot.len() - 1);
            }
            Ok(())
        }
    }
}

/// Test whether the element at `index` structurally equals `needle`.
fn structural_element_eq(target: &ArrayLikeRef<'_>, index: usize, needle: &ItemRs) -> bool {
    match target {
        ArrayLikeRef::Array(arr) => arr
            .get(index)
            .is_some_and(|v| equality::item_value_eq(needle, v)),
        ArrayLikeRef::Aot(aot) => aot
            .get(index)
            .is_some_and(|t| equality::item_table_eq(needle, t)),
    }
}

/// Append `n` copies of `source`'s elements to `dest`, preserving the
/// formatting (indentation, block and inline comments) of every cloned
/// element including the source's last-element inline comment — which
/// lives in the array's trailing slot and would otherwise be lost.
///
/// Returns `false` if `dest` and `source` are of different kinds (array
/// vs array-of-tables), leaving `dest` unchanged.
pub(crate) fn clone_elements_into(dest: &mut ItemRs, source: ArrayLikeRef<'_>, n: usize) -> bool {
    match (dest, source) {
        (ItemRs::Value(ValueRs::Array(dest_arr)), ArrayLikeRef::Array(src_arr)) => {
            clone_array_elements(dest_arr, src_arr, n);
            true
        }
        (ItemRs::ArrayOfTables(dest_aot), ArrayLikeRef::Aot(src_aot)) => {
            for _ in 0..n {
                for t in src_aot.iter() {
                    let mut t = t.clone();
                    // Clear source position so toml_edit renders the cloned
                    // entry in push order rather than interleaving it by span.
                    t.set_position(None);
                    dest_aot.push(t);
                    fix_inserted_aot_spacing(dest_aot, dest_aot.len() - 1);
                }
            }
            true
        }
        _ => false,
    }
}

fn clone_array_elements(dest_arr: &mut toml_edit::Array, src_arr: &toml_edit::Array, n: usize) {
    let src_inlines = src_arr.save_inline_comments();
    with_preserved_seam(dest_arr, |dest_arr, ic| {
        for _ in 0..n {
            for (src_i, v) in src_arr.iter().enumerate() {
                let mut v = v.clone();
                // At a single-line seam, inject the destination's inter-element
                // separator — source element 0 has no leading separator of its own.
                if src_i == 0
                    && let Some(sep) = single_line_separator(dest_arr)
                {
                    v.decor_mut().set_prefix(sep);
                }
                dest_arr.push_formatted(v);
                ic.push(src_inlines[src_i].clone());
            }
        }
    });
}

/// Append `extra` additional copies of `item`'s own elements to itself.
///
/// Used by `__mul__` and `__imul__`; callers should skip the call when
/// `extra == 0` rather than relying on an internal short-circuit.
pub(crate) fn item_repeat(item: &mut ItemRs, extra: usize, op: &str) -> PyResult<()> {
    let source = item.clone();
    let src_ref = as_array_like(&source, op)?;
    clone_elements_into(item, src_ref, extra);
    Ok(())
}

/// Check if any element structurally equals `needle`.
pub(crate) fn contains_structural(target: ArrayLikeRef<'_>, needle: &ItemRs) -> bool {
    (0..target.len()).any(|i| structural_element_eq(&target, i, needle))
}

/// Count how many elements structurally equal `needle`.
pub(crate) fn item_count_structural(target: ArrayLikeRef<'_>, needle: &ItemRs) -> usize {
    (0..target.len())
        .filter(|&i| structural_element_eq(&target, i, needle))
        .count()
}

/// Find the index of the first element structurally equal to `needle`
/// within the range `[start, stop)`.
pub(crate) fn item_index_structural_range(
    target: ArrayLikeRef<'_>,
    needle: &ItemRs,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<usize> {
    let len = target.len();
    let start = clamp_index(start.unwrap_or(0), len);
    let stop = clamp_index(stop.unwrap_or(len as i64), len);
    for i in start..stop {
        if structural_element_eq(&target, i, needle) {
            return Ok(i);
        }
    }
    Err(PyValueError::new_err("value not in array"))
}

/// Find and remove the first element structurally equal to `needle`.
///
/// Returns the key/index affected for staleness tracking.
pub(crate) fn find_and_remove(item: &mut ItemRs, needle: &ItemRs) -> PyResult<Affected> {
    let index = {
        let target = as_array_like(item, "remove()")?;
        item_index_structural_range(target, needle, None, None)?
    };
    let target = as_array_like_mut(item, "remove()")?;
    let (_removed, affected) = item_remove_at(target, index)?;
    Ok(affected)
}

/// Format an array as multiline, with each element on its own line.
/// No-op on empty arrays.
pub(crate) fn item_set_multiline(target: ArrayLikeMut<'_>, indent: usize) -> PyResult<()> {
    match target {
        ArrayLikeMut::Array(arr) => {
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
        ArrayLikeMut::Aot(_) => Ok(()),
    }
}

/// Pop an element from an array-like item.
///
/// When `index` is `Some`, pops by index; when `None`, pops the last element.
/// The caller must resolve the index to `i64` **before** locking the document,
/// because `extract::<i64>()` can invoke Python's `__index__` protocol.
pub(crate) fn list_pop(target: ArrayLikeMut<'_>, index: Option<i64>) -> PyResult<(Item, Affected)> {
    let len = target.len();
    let idx = match index {
        Some(i) => resolve_index(i, len)?,
        None => {
            if len == 0 {
                return Err(PyIndexError::new_err("pop from empty array"));
            }
            len - 1
        }
    };
    item_remove_at(target, idx)
}

// ---------------------------------------------------------------------------
// Subscript key resolution (used by ListProxy)
// ---------------------------------------------------------------------------

/// A subscript key resolved from Python *before* locking the document.
///
/// Proxy values are resolved to their plain Python equivalents so that
/// `extract()` works without locking the document.
pub(crate) enum SubscriptKey<'py> {
    Int(i64),
    Slice(Bound<'py, PySlice>),
}

/// Resolve a `__getitem__`/`__setitem__`/`__delitem__` key to a typed enum.
///
/// This must be called *before* taking the document's write lock, because
/// resolving a proxy key reads the document.  Returns `TypeError` for non-integer,
/// non-slice keys.
pub(crate) fn resolve_subscript_key<'py>(
    py: Python<'py>,
    key: &Bound<'py, PyAny>,
) -> PyResult<SubscriptKey<'py>> {
    if let Ok(slice) = key.cast::<PySlice>() {
        return Ok(SubscriptKey::Slice(slice.clone()));
    }
    let resolved = crate::item_proxy::resolve_proxy(key)?;
    let resolved_key = resolved.as_ref().map_or(key, |v| v.bind(py));
    resolved_key
        .extract::<i64>()
        .map(SubscriptKey::Int)
        .map_err(|_| {
            let type_name = key
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "?".to_owned());
            PyTypeError::new_err(format!(
                "TOML array indices must be integers or slices, not {type_name}"
            ))
        })
}

// ---------------------------------------------------------------------------
// Setitem / Delitem (used by ListProxy)
// ---------------------------------------------------------------------------

/// Set an integer-keyed entry in an array or array of tables.
/// Returns the key of the replaced element.
pub(crate) fn item_setitem_int(
    target: ArrayLikeMut<'_>,
    idx_raw: i64,
    value: Item,
) -> PyResult<crate::item_ops::Key> {
    use crate::item_ops::Key;
    match target {
        ArrayLikeMut::Array(array) => {
            let idx = resolve_index(idx_raw, array.len())?;
            let mut v = into_value(value)?;
            let inline = comments::take_value_inline_comment(&mut v);
            array.replace(idx, v);
            if !inline.is_empty() {
                comments::set_array_inline_comment(array, idx, &inline);
            }
            Ok(Key::Int(idx))
        }
        ArrayLikeMut::Aot(aot) => {
            let idx = resolve_index(idx_raw, aot.len())?;
            let mut table = require_table(value)?;
            let saved = save_aot_entry_prefix(aot, idx);
            table.decor_mut().set_prefix(&saved);
            aot.replace(idx, table);
            Ok(Key::Int(idx))
        }
    }
}

/// Delete an integer-keyed entry from an array or array of tables.
pub(crate) fn item_delitem_int(item: &mut ItemRs, idx_raw: i64) -> PyResult<Affected> {
    let target = as_array_like_mut(item, "__delitem__")?;
    let idx = resolve_index(idx_raw, target.len())?;
    let (_removed, affected) = item_remove_at(target, idx)?;
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_delitem_slice_empty_array() {
        let mut arr = toml_edit::Array::new();
        // arr.len() - 1 underflows in debug builds when indices is empty
        item_delitem_slice(ArrayLikeMut::Array(&mut arr), &[]).unwrap();
        assert_eq!(arr.len(), 0);
    }
}
