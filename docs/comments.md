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

Comments on fields in inline tables require TOML 1.1. For TOML 1.0
compatibility, use a table header such as `[server]` instead of
`server = { ... }`.

A comment attached to an [implicit table](layout.md#implicit-tables) won't
appear in the output unless that table has a header. Give it one by setting
`table.implicit = False`.
