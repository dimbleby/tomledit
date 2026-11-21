# Comments

Use `.comment` for comment lines above an entry and `.inline_comment` for a
comment after the value on the same line. Include the `#` in the text, and
assign `None` to remove a comment. Reading an unset comment returns `None`.

```python exec="on" source="above" result="toml"
from tomledit import dumps, loads

doc = loads('version = "2.0.0"\n')
version = doc["version"]
version.comment = "# Bumped for release"
version.inline_comment = "# Keep in sync with the tag"

assert dumps(doc) == '# Bumped for release\nversion = "2.0.0" # Keep in sync with the tag\n'  # markdown-exec: hide
print(dumps(doc), end="")
```

Some TOML readers reject comments inside inline tables. If a field needs its
own comment, use a standard table (`[server]`) rather than an inline table
(`server = { ... }`) for compatibility.
