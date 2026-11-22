# tomledit

[![PyPI](https://img.shields.io/pypi/v/tomledit)](https://pypi.org/project/tomledit/)
[![Docs](https://img.shields.io/badge/docs-zensical-informational)](https://dimbleby.github.io/tomledit/)

A format-preserving TOML editor for Python, powered by Rust's
[toml_edit](https://docs.rs/toml_edit).

Parse a TOML document, modify it, and write it back \- comments, whitespace, and
ordering are preserved.

## Quick start

Save this as `example.toml`:

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

from tomledit import dump, load

path = Path("example.toml")
with path.open("rb") as f:
    doc = load(f)

doc["project"]["version"].comment = "# Bumped for release"
doc["project"]["version"] = "2.0.0"
doc["project"]["keywords"].append("important-keyword")
doc["project"]["keywords"].inline_comment = "# updated"

with path.open("wb") as f:
    dump(doc, f)
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
from tomledit import loads

doc = loads('arr = ["a", "b"]')
first = doc["arr"][0]         # `first` mostly behaves as "a"
doc["arr"][0] = "changed"     # prefer `first` still to be "a", but...
print(first)                  # RuntimeError: this Item is stale
```

Changes in unrelated parts of the document are no problem:

```python
first = doc["arr"][0]
doc["foo"] = "bar"
print(first)                  # this is fine
```

If you just need the plain Python value, grab it with `.value`:

```python
from tomledit import loads

doc = loads('arr = ["a", "b"]')
first = doc["arr"][0].value   # "a" - a regular string, not a path
doc["arr"][0] = "changed"
print(first)                  # still "a"
```

For many usage patterns, stale `Item`s will never be an issue.
If they become a problem for you, you might prefer
[tomlrt](https://pypi.org/project/tomlrt/).

## Documentation

Full docs at <https://dimbleby.github.io/tomledit/>.
