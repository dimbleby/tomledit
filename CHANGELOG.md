# Changelog

## Unreleased

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
