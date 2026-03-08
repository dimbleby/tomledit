# tomledit

A format-preserving TOML editor for Python, powered by Rust's
[toml_edit](https://docs.rs/toml_edit) via [pyo3](https://pyo3.rs).

Parse a TOML document, modify it like a native Python dict, and write it back
— comments, whitespace, and ordering are preserved.

Set and remove comments in the document.

## Quick start

```python
from tomledit import Document

doc = Document.parse(open("pyproject.toml").read())

doc["project"]["version"].comment = "# Version 2"
doc["project"]["version"] = "2.0.0"
doc["project"]["keywords"].append("important-keyword")
doc["project"]["keywords"].inline_comment = "# keywords"
del doc["project"]["optional-dependencies"]

open("pyproject.toml", "w").write(str(doc))
```
