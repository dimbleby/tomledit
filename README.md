# tomledit

[![PyPI](https://img.shields.io/pypi/v/tomledit)](https://pypi.org/project/tomledit/)

A format-preserving TOML editor for Python, powered by Rust's
[toml_edit](https://docs.rs/toml_edit).

Parse a TOML document, modify it, and write it back \- comments, whitespace, and
ordering are preserved.

## Quick start

Given a TOML file like this:

```toml
[project]
name = "my-app"
version = "1.0.0"
# Search terms for the package index
keywords = ["python", "toml"]

[project.optional-dependencies]
dev = ["pytest"]
```

You can parse, modify, and write it back \- comments, whitespace, and
ordering are all preserved:

```python
from pathlib import Path

from tomledit import Document

text = Path("pyproject.toml").read_text(encoding="utf-8")
doc = Document.parse(text)

doc["project"]["version"].comment = "# Bumped for release"
doc["project"]["version"] = "2.0.0"
doc["project"]["keywords"].append("important-keyword")
doc["project"]["keywords"].inline_comment = "# updated"

Path("pyproject.toml").write_text(doc.as_toml(), encoding="utf-8")
```

The result:

```toml
[project]
name = "my-app"
# Bumped for release
version = "2.0.0"
# Search terms for the package index
keywords = ["python", "toml", "important-keyword"] # updated

[project.optional-dependencies]
dev = ["pytest"]
```

## Truth in advertising

It is intended that using a `Document` feels just like using a native Python
dictionary, and that the `Item`s you get from it can also be treated as ordinary
dictionaries and lists and suchlike.

Under the hood, though, every `Item` is really a _path_ back into the shared
document.

This is mostly invisible, but sometimes the implementation leaks out.
Because items are paths, a mutation to the `Document` can invalidate or change
the value being pointed at.
In such cases the `Item` is made stale:

```python
doc = Document.parse('arr = ["a", "b"]')
first = doc["arr"][0]       # `first` mostly behaves as "a"
doc["arr"][0] = "changed"   # prefer `first` still to be "a", but...
print(first)                # RuntimeError: this Item is stale
```

The item that performs the mutation stays valid:

```python
arr = doc["arr"]
arr[0] = "changed"
print(arr)                  # works as expected
```

Changes in unrelated parts of the document are no problem:

```python
arr = doc["arr"]
doc["foo"] = "bar"
print(arr)                  # this is fine
```

If you just need the plain Python value, grab it with `.value` before the
document changes:

```python
first = doc["arr"][0].value   # "a" - a plain str, not a path
doc["arr"][0] = "changed"
print(first)                  # still "a"
```
