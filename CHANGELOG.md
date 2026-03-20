# Changelog

## Unreleased

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
