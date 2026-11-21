# Working with items

Most of the time, you can work with TOML data as if it were ordinary Python
data: tables behave like dictionaries, arrays like lists, and scalars like
strings, numbers, and other Python values. Index a table, append to an array,
or compare a scalar directly:

```python exec="on" source="above" session="items"
from tomledit import loads

doc = loads('[project]\nversion = "1.0"\ndependencies = ["requests"]\n')
project = doc["project"]
project["dependencies"].append("httpx")
assert project["version"] == "1.0"
```

In fact, a lookup returns an [`Item`][tomledit.Item]: a
[`DictItem`][tomledit.DictItem], [`ListItem`][tomledit.ListItem], or
[`ScalarItem`][tomledit.ScalarItem]. An item refers to a location in the
document rather than holding a copy.

TOML has no `None` value. To remove an entry, use `del`; if you use
`setdefault` to insert a missing key, supply a TOML value as the default.

## When items become stale

Replacing or removing an entry makes existing references to it stale. Replacing
a whole table also makes references to entries inside it stale. In an array,
inserting or removing an element can make references to later elements stale
because their indices shift. Unrelated edits leave an item usable.

Using a stale item raises `RuntimeError`. Look it up again if you want to work
with whatever is at that location now:

```python exec="on" source="above" session="items"
version = project["version"]
original_version = version.value

project["dependencies"].append("rich")
assert version == "1.0"

project["version"] = "2.0"
# Using version now would raise RuntimeError.
assert project["version"] == "2.0"
assert original_version == "1.0"
```

## Copying an item

Assigning an item to another entry copies its TOML representation rather than
making both entries refer to the same value:

```python exec="on" source="above" session="items"
doc["backup"] = project
doc["backup"]["version"] = "3.0"
assert project["version"] == "2.0"
```

## Getting plain Python values

Use `.value` when you need a snapshot that won't change with the document, as
with `original_version` above. A table or array's `.value` converts its nested
entries too. Use `doc.value` for the whole document as a dictionary.

Use `.value` also when passing an item to an API that expects a native Python
value, such as `json.dumps()`.
