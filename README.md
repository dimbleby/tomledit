# tomledit

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

Path("pyproject.toml").write_text(str(doc), encoding="utf-8")
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
In particular: because items are paths, they can go stale when the document
changes underneath them.

```python
doc = Document.parse('arr = ["a", "b"]')
first = doc["arr"][0]       # a path to arr[0]
doc["arr"][0] = "changed"   # mutates the document
print(first)                # RuntimeError: this Item is stale
```

The item that _performs_ the mutation stays valid, so chaining works fine:

```python
arr = doc["arr"]
arr.append("c")
arr.append("d")   # still works - arr itself did the mutating
```

If you just need the plain Python value, grab it with `.value` before the
document changes:

```python
first = doc["arr"][0].value   # "a" - a plain str, not a path
doc["arr"][0] = "changed"
print(first)                  # still "a"
```
