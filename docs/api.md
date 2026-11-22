# API reference

Import these functions and classes from `tomledit`. For examples of ordinary
dictionary and list operations, see [Working with items](working-with-items.md).

## Reading and writing

::: tomledit.load
::: tomledit.loads
::: tomledit.dump
::: tomledit.dumps

## Documents and items

::: tomledit.Document
    options:
      members: [as_toml, value, fmt]

::: tomledit.Item
    options:
      members: [parse, as_toml, value, comment, inline_comment, fmt]

::: tomledit.DictItem
    options:
      members: [implicit]

::: tomledit.ListItem
    options:
      members: [set_multiline]

::: tomledit.ScalarItem
    options:
      members: false
