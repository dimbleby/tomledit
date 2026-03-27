use pyo3::exceptions::{PyIndexError, PyKeyError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use toml_edit::Item as ItemRs;
use toml_edit::Value as ValueRs;

use crate::comments;
use crate::equality;
use crate::item::Item;
use crate::item_ops::{Key, into_value};

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
    // Inserting at the front: the element that needs fixing is the old first
    // element, now sitting at position 1.
    let target = if pos == 0 { 1 } else { pos };
    if target >= aot.len() {
        return;
    }
    // Detect whether the nearest non-first neighbour uses blank-line spacing.
    // Prefer the element *before* — elements after may also be newly pushed
    // and not yet corrected.
    //
    // A `None` prefix means "unset" — toml_edit will insert a default blank
    // line for non-first AoT entries, so we treat it as spaced.  Only an
    // explicitly set prefix that does NOT start with '\n' counts as compact.
    let spaced = [target - 1, target + 1]
        .into_iter()
        .filter(|&i| i > 0 && i < aot.len())
        .find_map(|i| {
            aot.iter().nth(i).map(|t| {
                let as_str = t.decor().prefix().and_then(|r| r.as_str());
                // None → default blank line (spaced)
                // Some(s) starting with '\n' → explicitly spaced
                // Some(s) not starting with '\n' → compact
                as_str.is_none() || as_str.is_some_and(|s| s.starts_with('\n'))
            })
        });
    let Some(spaced) = spaced else { return };

    // Read the target's current prefix.  Distinguish between `None` (unset —
    // toml_edit will insert a default blank line) and `Some("")` (explicitly
    // empty — no blank line).
    let raw_prefix = aot
        .iter()
        .nth(target)
        .and_then(|t| t.decor().prefix()?.as_str().map(str::to_owned));
    let current = raw_prefix.as_deref().unwrap_or("");
    let has_blank = current.starts_with('\n');

    if spaced && !has_blank {
        // Prepend a blank line, preserving any existing content (comments).
        aot.iter_mut()
            .nth(target)
            .expect("target in bounds")
            .decor_mut()
            .set_prefix(format!("\n{current}"));
    } else if !spaced && (raw_prefix.is_none() || current == "\n") {
        // Prefix is either unset (toml_edit would insert a default blank
        // line) or is a bare "\n".  Explicitly set to "" to suppress it.
        aot.iter_mut()
            .nth(target)
            .expect("target in bounds")
            .decor_mut()
            .set_prefix("");
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
    match item {
        ItemRs::Value(ValueRs::Array(arr)) => resolve_index(index, arr.len()),
        ItemRs::ArrayOfTables(aot) => resolve_index(index, aot.len()),
        ItemRs::Table(_) | ItemRs::Value(ValueRs::InlineTable(_)) => {
            Err(PyKeyError::new_err(index.to_string()))
        }
        _ => Err(PyTypeError::new_err(format!(
            "TOML {} item is not subscriptable (use .value to get the Python object)",
            item.type_name()
        ))),
    }
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
            let mut ic = comments::save_inline_comments(arr);
            let removing_first = sorted.last() == Some(&0);
            let removing_last = !sorted.is_empty() && sorted.first() == Some(&(arr.len() - 1));
            let decor = save_removal_decor(arr, removing_first, removing_last);
            for idx in sorted {
                arr.remove(idx);
                ic.remove(idx);
            }
            comments::restore_inline_comments(arr, &ic);
            apply_removal_decor(arr, &decor);
            Ok(())
        }
        ArrayLikeMut::Aot(aot) => {
            for idx in sorted {
                aot.remove(idx);
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

        let mut ic = comments::save_inline_comments(arr);
        let removes_first = start_idx == 0 && stop_idx > 0;
        let removes_last = stop_idx == arr.len() && stop_idx > start_idx;
        let decor = save_removal_decor(
            arr,
            removes_first && converted.is_empty(),
            removes_last && converted.is_empty(),
        );

        // Remove old elements from back to front.
        for i in (start_idx..stop_idx).rev() {
            arr.remove(i);
            ic.remove(i);
        }

        // Insert new elements at start position.
        for (offset, mut v) in converted.into_iter().enumerate() {
            let inline = comments::take_inline_comment(&mut v);
            let idx = start_idx + offset;
            if idx >= arr.len() {
                arr.push(v);
            } else {
                arr.insert(idx, v);
            }
            ic.insert(idx, inline);
        }
        comments::restore_inline_comments(arr, &ic);
        apply_removal_decor(arr, &decor);
        Ok(())
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
            let inline = comments::take_inline_comment(&mut v);
            arr.replace(idx, v);
            if !inline.is_empty() {
                comments::set_array_item_comment(arr, idx, &inline);
            }
        }
        Ok(())
    }
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
    let tables: Vec<toml_edit::Table> = values
        .into_iter()
        .map(require_table)
        .collect::<PyResult<_>>()?;

    if step == 1 {
        let start_idx = start as usize;
        let stop_idx = stop as usize;
        let tables_count = tables.len();
        for i in (start_idx..stop_idx).rev() {
            aot.remove(i);
        }
        for (offset, table) in tables.into_iter().enumerate() {
            aot.insert(start_idx + offset, table);
        }
        for i in start_idx..start_idx + tables_count {
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
        for (idx, table) in indices.into_iter().zip(tables) {
            aot.remove(idx);
            aot.insert(idx, table);
            fix_inserted_aot_spacing(aot, idx);
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Array boundary-element decoration repair
// ---------------------------------------------------------------------------

/// Decoration state captured before an array removal so that the opening and
/// closing brackets stay in their original positions.
pub(crate) struct RemovalDecor {
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
pub(crate) fn save_removal_decor(
    arr: &toml_edit::Array,
    removing_first: bool,
    removing_last: bool,
) -> RemovalDecor {
    let at_least_two = arr.len() >= 2;
    RemovalDecor {
        first_prefix: (removing_first && at_least_two).then(|| {
            arr.get(0)
                .and_then(|v| v.decor().prefix().and_then(|r| r.as_str()))
                .unwrap_or_default()
                .to_owned()
        }),
        last_suffix: (removing_last && at_least_two).then(|| {
            let last = arr.len() - 1;
            arr.get(last)
                .and_then(|v| v.decor().suffix().and_then(|r| r.as_str()))
                .unwrap_or_default()
                .to_owned()
        }),
    }
}

/// Apply saved decoration fixes after a removal + `restore_inline_comments`.
pub(crate) fn apply_removal_decor(arr: &mut toml_edit::Array, decor: &RemovalDecor) {
    // --- First-element prefix ---
    // The new first element inherits a prefix meant to follow a comma (e.g.
    // `" "` in `[1, 2]` or `" # note\n    "` in multiline arrays).  Replace
    // it with the old first element's prefix so `[1, 2, 3]` becomes `[2, 3]`
    // instead of `[ 2, 3]`.
    if let Some(ref old_first_prefix) = decor.first_prefix
        && let Some(new_first) = arr.get_mut(0)
    {
        let cur = new_first
            .decor()
            .prefix()
            .and_then(|r| r.as_str())
            .unwrap_or_default()
            .to_owned();
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
            let mut ic = comments::save_inline_comments(arr);
            let mut v = into_value(value)?;
            let inline = comments::take_inline_comment(&mut v);
            apply_multiline_decor(arr, &mut v);
            arr.push(v);
            ic.push(inline);
            comments::restore_inline_comments(arr, &ic);
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

/// Insert an element.  Returns `true` when the insertion was at the end
/// (append-like, no index shifting occurred).
pub(crate) fn item_insert(target: ArrayLikeMut<'_>, index: i64, value: Item) -> PyResult<bool> {
    match target {
        ArrayLikeMut::Array(arr) => {
            let resolved = clamp_index(index, arr.len());
            let at_end = resolved == arr.len();
            let mut ic = comments::save_inline_comments(arr);
            let mut v = into_value(value)?;
            let inline = comments::take_inline_comment(&mut v);
            apply_multiline_decor(arr, &mut v);
            arr.insert(resolved, v);
            ic.insert(resolved, inline);
            comments::restore_inline_comments(arr, &ic);
            Ok(at_end)
        }
        ArrayLikeMut::Aot(aot) => {
            let resolved = clamp_index(index, aot.len());
            let at_end = resolved == aot.len();
            let table = require_table(value)?;
            aot.insert(resolved, table);
            fix_inserted_aot_spacing(aot, resolved);
            Ok(at_end)
        }
    }
}

/// Remove the element at `index`.  Returns `Some(Key::Int(idx))` when
/// only the last element was removed (no shifting), or `None` when
/// earlier indices shifted and the whole container must be invalidated.
pub(crate) fn item_remove_at(target: ArrayLikeMut<'_>, index: usize) -> PyResult<Option<Key>> {
    match target {
        ArrayLikeMut::Array(arr) => {
            if index >= arr.len() {
                return Err(PyIndexError::new_err("array index out of range"));
            }
            let mut ic = comments::save_inline_comments(arr);
            let mut decor = save_removal_decor(arr, true, true);
            let last = arr.len() - 1;
            if index != 0 {
                decor.first_prefix = None;
            }
            if index != last {
                decor.last_suffix = None;
            }
            arr.remove(index);
            ic.remove(index);
            comments::restore_inline_comments(arr, &ic);
            apply_removal_decor(arr, &decor);
            Ok((index == last).then_some(Key::Int(index)))
        }
        ArrayLikeMut::Aot(aot) => {
            if index >= aot.len() {
                return Err(PyIndexError::new_err("array index out of range"));
            }
            let last = aot.len() - 1;
            aot.remove(index);
            Ok((index == last).then_some(Key::Int(index)))
        }
    }
}

pub(crate) fn item_extend(target: ArrayLikeMut<'_>, items: Vec<Item>) -> PyResult<()> {
    match target {
        ArrayLikeMut::Array(arr) => {
            // Validate all values up front.
            let converted: Vec<ValueRs> =
                items.into_iter().map(into_value).collect::<PyResult<_>>()?;
            let mut ic = comments::save_inline_comments(arr);
            for mut v in converted {
                let inline = comments::take_inline_comment(&mut v);
                apply_multiline_decor(arr, &mut v);
                arr.push(v);
                ic.push(inline);
            }
            comments::restore_inline_comments(arr, &ic);
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

pub(crate) fn item_count(target: ArrayLikeRef<'_>, value: &Bound<'_, PyAny>) -> PyResult<usize> {
    match target {
        ArrayLikeRef::Array(arr) => {
            let mut count = 0;
            for v in arr.iter() {
                if equality::value_eq(v, value)? {
                    count += 1;
                }
            }
            Ok(count)
        }
        ArrayLikeRef::Aot(aot) => {
            let mut count = 0;
            for table in aot.iter() {
                if equality::table_eq(table, value)? {
                    count += 1;
                }
            }
            Ok(count)
        }
    }
}

pub(crate) fn item_index(
    target: ArrayLikeRef<'_>,
    value: &Bound<'_, PyAny>,
    start: Option<i64>,
    stop: Option<i64>,
) -> PyResult<usize> {
    match target {
        ArrayLikeRef::Array(arr) => {
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
            Err(PyValueError::new_err("value not in array"))
        }
        ArrayLikeRef::Aot(aot) => {
            let len = aot.len();
            let start = clamp_index(start.unwrap_or(0), len);
            let stop = clamp_index(stop.unwrap_or(len as i64), len);
            for i in start..stop {
                if let Some(table) = aot.get(i)
                    && equality::table_eq(table, value)?
                {
                    return Ok(i);
                }
            }
            Err(PyValueError::new_err("value not in array"))
        }
    }
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
