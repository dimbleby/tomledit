# TOML layout

## Tables and arrays of tables

You edit an inline table (`server = { port = 80 }`) and a standard table
(`[server]`) the same way: `doc["server"]["port"] = 8080` keeps the existing
table form. A nonempty list of dictionaries assigned at the document root is
written as an array of tables (`[[servers]]`). An array of tables accepts only
tables as elements, even though it behaves like a list.

## Implicit tables

A header like `[server.tls]` creates a parent table `server` even if there is
no `[server]` header. You can access that table as `doc["server"]`. Set
[`implicit`][tomledit.DictItem.implicit] to `False` to give it its own header:

```python exec="on" source="above" result="toml"
from tomledit import dumps, loads

doc = loads("[server.tls]\nenabled = true\n")
assert doc["server"].implicit

doc["server"].implicit = False
assert dumps(doc).startswith("[server]\n[server.tls]\n")  # markdown-exec: hide
print(dumps(doc), end="")
```

Set `implicit` to `True` to omit the header when the table has no values of
its own. This setting does not affect inline tables.

## Reformatting

Use [`set_multiline()`][tomledit.ListItem.set_multiline] to put each element of
an ordinary array on its own line while keeping comments. Use
[`fmt()`][tomledit.Item.fmt] to normalize formatting, for example to remove
line breaks between array elements. `fmt()` can remove comments inside the
array. Neither operation reformats an array of tables.
