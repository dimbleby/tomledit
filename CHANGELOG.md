# Changelog

## Unreleased

- Fix slice assignment collapsing a multiline array onto one line; new
  elements now keep the array's existing layout.

## 1.2.0 (17 June 2026)

- Add module-level `load`, `loads`, `dump`, and `dumps` functions.
- Fix inline and block comments in "leading-comma" array/inline-table layouts
  (where the comma starts the next line), including reads, edits, and
  structural mutations.

## 1.1.4 (19 April 2026)

- New entries appended/inserted into an array of tables now copy the indent
  style of their neighbours (both the header indent and the per-key body
  indent), instead of always being emitted at column 0.
- New keys added to a regular table now copy the indent of existing sibling
  keys, instead of always being emitted at column 0.

## 1.1.3 (19 April 2026)

- Fix a TOCTOU window in `DictItem.update()` between proxy freshness check
  and document write-lock acquisition that could let a stale proxy mutate
  post-mutation state under free-threading.
- Use `pyo3::sync::RwLockExt` to avoid possible deadlocks

## 1.1.2 (18 April 2026)

- Fix consistency of reads in free-threaded builds

## 1.1.1 (16 April 2026)

- Fix `ListItem` concatenation (`+`, `+=`) losing inline comments at the
  seam when the left array was multi-line.
- Fix `arr += other` dropping formatting when `other` is a `ListItem` from
  the same document.
- Fix `aot *= n` losing blank-line spacing at the seam between the original
  entries and the repeated copies.
- Fix AoT operations (`append`, `insert`, `extend`, `+=`) interleaving
  entries by source span when the source tables came from another parsed
  document.  Cloned table positions are now cleared so rendering follows
  push order.
- Fix `KeysView` missing reflected set operators: `set | keys`, `set & keys`,
  `set - keys`, `set ^ keys` now work, matching `dict.keys()`.
- Fix `ItemsView` missing set operators entirely: `|`, `&`, `-`, `^` and
  their reflected forms now work, matching `dict.items()`.  Unhashable
  values raise `TypeError`, as they do with `dict_items`.
- Fix a TOCTOU window between proxy freshness check and document lock
  acquisition that could let a stale proxy observe post-mutation state
  under free-threading.  All proxy methods now take the lock first, then
  verify freshness under the lock.

## 1.1.0 (16 April 2026)

- Use frozen pyclasses with `RwLock` internally, allowing concurrent reads
  in free-threaded Python builds instead of serializing all access.

## 1.0.0 (3 April 2026)

- New `ListItem` operations: `+`, `*`, and `*=` for list concatenation and
  repetition, mirroring Python's `list` interface.
- PEP 584 merge operators: `|` and `|=` on `Document` and `DictItem` for
  dict-style merging, plus `popitem()`.
- Array-of-tables improvements: `replace()`, `insert()`, inline comments,
  and better spacing/comment preservation throughout AoT mutations.
- `DictItem.implicit` property to get or set whether a table's header is
  suppressed in TOML output (e.g. `[a]` is implicit when only `[a.b]` is
  defined).
- Broader equality and containment: inline tables now compare equal to any
  `Mapping`; cross-type proxy equality (e.g. `Table` vs `InlineTable`) works
  correctly.
- Comment robustness: block comments survive type changes, TOML-invalid
  control characters are rejected, and comments/formatting are preserved across
  `update()` and merge operations.
- Many double-borrow panic fixes — proxy keys, `update(self)`, `|= self`,
  `ListItem` self-references, `pop` with proxy keys, and view operations no
  longer trigger RefCell panics.
- More precise proxy invalidation: array mutations now only invalidate
  the affected elements rather than the entire array; views now check
  freshness.
- Free-threaded Python wheels are now available.

## 0.12.1 (20 March 2026)

- Fix `ListItem.remove()` panic when passed an `Item` proxy (e.g.
  a `ScalarItem` obtained by indexing).
  Also fix `__contains__`, `count()`, and `index()` returning wrong results for
  non-integer proxy arguments.

## 0.12.0 (20 March 2026)

- Proxy invalidation is now path-precise.
  Mutating one part of a document no longer invalidates `Item` references to
  unrelated subtrees.
- `DictItem.parse()`, `ListItem.parse()`, and `ScalarItem.parse()` now validate
  that the parsed value matches the expected type, raising `ValueError` on
  mismatch.

## 0.11.0 (19 March 2026)

- **Breaking:** `str(Document)` now returns a Python dict-like representation
  instead of TOML text.
  Use the new `as_toml()` method on `Document` and `Item` for TOML serialisation.
- Support `__contains__` on `ScalarItem` (for strings).

## 0.10.0 (18 March 2026)

- Split `Item` into `DictItem`, `ListItem`, and `ScalarItem` subclasses.
  `isinstance` checks and `MutableMapping`/`MutableSequence` protocols now work.
- Forward arithmetic, comparison, and type-conversion dunder methods on
  `ScalarItem` to the underlying Python value.
- Add `ListItem.set_multiline()` to format an array with one element per line.
- Preserve multiline formatting when appending, inserting, or extending arrays.
- Add `KeysView`, `ValuesView`, `ItemsView` for dict-like views.
- Add missing methods: `setdefault()`, `get()` with default, array-of-tables
  indexing and iteration.
- Prefer standard `[table]` over inline tables when assigning dicts.

## 0.9.5 (15 March 2026)

- Fix insertion of dictionaries into inline tables

## 0.9.4 (15 March 2026)

- Support `.inline_comment` on inline table values (read, write, preserve across mutations).
- Fix `.comment` on inline table keys being misattributed to the wrong key.

## 0.9.3 (14 March 2026)

- Fix inline comments on array elements being displaced by mutations (append,
  insert, remove, pop, del, slice assignment, extend).

## 0.9.2 (14 March 2026)

- Fix `.comment` on tables.

## 0.9.1 (13 March 2026)

- Assigning a dict now creates a standard `[table]`, not an inline table.

## 0.9.0 (13 March 2026)

- Support more list-like methods: `index()`, `count()`, `__iadd__()`.
- Fix equality comparisons for `datetime.date` and `datetime.time` values.
- Faster `str()` conversion for scalar items (strings, ints, floats, bools).
- Allow assigning `datetime.date` and `datetime.time` values.
- Make `pop()` behave more like Python's built-ins.
- Be less eager to invalidate `Item`s after additive `update()` calls.
- Surface timezone-conversion errors when assigning `datetime` values.

## 0.8.0 (10 March 2026)

- Make our dictionary-like methods even more dictionary-like

## 0.7.2 (10 March 2026)

- Better docstrings

## 0.7.1 (9 March 2026)

- Fix type stubs: reinstate `__eq__`

## 0.7.0 (9 March 2026)

- Improved error reporting for wrong keys and wrong types of keys

## 0.6.0 (8 March 2026)

- Be less aggressive about invalidating `Item`s
  - eg appending to an array cannot invalidate any existing path
- Reject integer keys in tables
- Allow floats and integers to be equal
- Implement `__copy__` and `__deepcopy__` on `Document`

## 0.5.0 (8 March 2026)

- Add `Item.parse()`
- Allow assigning `Documents`s back into the document (e.g. `doc1["a"] = doc2["b"]`)

## 0.4.0 (8 March 2026)

- Allow assigning `Item`s back into the document (e.g. `doc["b"] = doc["a"]`)

## 0.3.0 (8 March 2026)

- Invalidate `Item`s when the document changes
- Remove `TomlError`
- Bugfix inserting into inline table

## 0.2.0 (8 March 2026)

- Add support for manipulating comments

## 0.1.1 (7 March 2026)

- Initial release
