# tomledit

A format-preserving TOML editor for Python, powered by Rust's
[toml_edit](https://docs.rs/toml_edit) via [pyo3](https://pyo3.rs).

Parse a TOML document, modify it like a native Python dict, and write it back
— comments, whitespace, and ordering are preserved.

Set and remove comments in the document.

## Quick start

```python
from pathlib import Path

from tomledit import Document

text = Path("pyproject.toml").read_text(encoding="utf-8")
doc = Document.parse(text)

doc["project"]["version"].comment = "# Version 2"
doc["project"]["version"] = "2.0.0"
doc["project"]["keywords"].append("important-keyword")
doc["project"]["keywords"].inline_comment = "# keywords"
del doc["project"]["optional-dependencies"]

Path("pyproject.toml").write_text(str(doc), encoding="utf-8")
```

## How Items work

When you index a `Document` or an `Item`, the returned `Item` is really a path
back into the document.

Because an `Item` is a path, modifying the document can invalidate previously
obtained items.
When that happens, accessing the stale `Item` raises a `RuntimeError`:

```python
doc = Document.parse('arr = ["a", "b"]')
first = doc["arr"][0]       # a path to arr[0]
doc["arr"][0] = "changed"   # modifies the document
print(first)                # RuntimeError: this Item is stale
```

The `Item` that performs the mutation remains valid, so you can keep using it:

```python
arr = doc["arr"]
arr.append("c")
arr.append("d")   # still works
```

If what you want is just the Python value, obtain it with `.value`:

```python
first = doc["arr"][0].value   # "a" — a plain Python str
doc["arr"][0] = "changed"
print(first)                  # still "a"
```
