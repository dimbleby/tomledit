# Quickstart

Save this as `example.toml`:

```python exec="on" result="toml" session="quickstart"
sample = '''\
[project]
name = "my-app"
version = "1.0.0"
# Search terms for the package index
keywords = ["python", "toml"]
'''
print(sample, end="")
```

Read the file with [`load`][tomledit.load], edit the document like a dictionary,
and write it back with [`dump`][tomledit.dump]:

```python exec="on" source="above" result="toml" session="quickstart"
from pathlib import Path

from tomledit import dump, load

path = Path("example.toml")
from tempfile import TemporaryDirectory  # markdown-exec: hide
workspace = TemporaryDirectory()  # markdown-exec: hide
path = Path(workspace.name) / path  # markdown-exec: hide
path.write_text(sample, encoding="utf-8")  # markdown-exec: hide
with path.open("rb") as f:
    doc = load(f)

project = doc["project"]

project["version"] = "2.0.0"
project["keywords"].append("important-keyword")

with path.open("wb") as f:
    dump(doc, f)

output = path.read_text(encoding="utf-8")  # markdown-exec: hide
assert '# Search terms for the package index\nkeywords = ["python", "toml", "important-keyword"]\n' in output  # markdown-exec: hide
assert 'version = "2.0.0"\n' in output  # markdown-exec: hide
print(output, end="")  # markdown-exec: hide
workspace.cleanup()  # markdown-exec: hide
```

Use [`loads`][tomledit.loads] and [`dumps`][tomledit.dumps] when working with
strings instead of files.
