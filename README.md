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
