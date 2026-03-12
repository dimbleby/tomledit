"""Tests for reading/writing scalars, arrays, keys, and format preservation."""

from __future__ import annotations

import zoneinfo
from datetime import datetime

import pytest

from tomledit import Document, Item

# ---------------------------------------------------------------------------
# Reading scalar values (like dict access)
# ---------------------------------------------------------------------------


class TestReadScalars:
    def test_top_level_string(self, doc: Document) -> None:
        assert doc["title"] == "Example"

    def test_nested_string(self, doc: Document) -> None:
        assert doc["owner"]["name"] == "Alice"

    def test_nested_int(self, doc: Document) -> None:
        assert doc["owner"]["age"] == 30

    def test_nested_bool(self, doc: Document) -> None:
        assert doc["owner"]["active"] == True  # noqa: E712

    def test_deeply_nested(self, doc: Document) -> None:
        assert doc["servers"]["alpha"]["ip"] == "10.0.0.1"
        assert doc["servers"]["beta"]["role"] == "backend"


# ---------------------------------------------------------------------------
# Reading array elements (like list access)
# ---------------------------------------------------------------------------


class TestReadArrays:
    def test_array_element_by_index(self, doc: Document) -> None:
        assert doc["database"]["ports"][0] == 8001
        assert doc["database"]["ports"][2] == 8002

    def test_array_out_of_bounds(self, doc: Document) -> None:
        with pytest.raises(IndexError):
            doc["database"]["ports"][99]


# ---------------------------------------------------------------------------
# Writing scalar values
# ---------------------------------------------------------------------------


class TestWriteScalars:
    def test_set_top_level_string(self, doc: Document) -> None:
        doc["title"] = "Changed"
        assert doc["title"] == "Changed"

    def test_set_nested_int(self, doc: Document) -> None:
        doc["owner"]["age"] = 31
        assert doc["owner"]["age"] == 31

    def test_set_nested_bool(self, doc: Document) -> None:
        doc["owner"]["active"] = False
        assert doc["owner"]["active"] == False  # noqa: E712

    def test_set_nested_float(self, doc: Document) -> None:
        doc["database"]["connection_max"] = 9.81
        assert doc["database"]["connection_max"] == 9.81

    def test_set_deeply_nested_string(self, doc: Document) -> None:
        doc["servers"]["alpha"]["ip"] = "192.168.1.1"
        assert doc["servers"]["alpha"]["ip"] == "192.168.1.1"

    def test_mutation_persists_in_str(self, doc: Document) -> None:
        doc["owner"]["name"] = "Bob"
        assert 'name = "Bob"' in str(doc)
        assert doc["owner"]["name"] == "Bob"


# ---------------------------------------------------------------------------
# Writing array elements (like list assignment)
# ---------------------------------------------------------------------------


class TestWriteArrayElements:
    def test_replace_array_element(self, doc: Document) -> None:
        doc["database"]["ports"][0] = 9999
        assert doc["database"]["ports"][0] == 9999

    def test_replace_does_not_affect_others(self, doc: Document) -> None:
        doc["database"]["ports"][0] = 9999
        assert doc["database"]["ports"][2] == 8002


# ---------------------------------------------------------------------------
# Setting new keys (like dict[key] = value)
# ---------------------------------------------------------------------------


class TestSetNewKeys:
    def test_add_string(self) -> None:
        doc = Document.parse("")
        doc["name"] = "hello"
        assert doc["name"] == "hello"

    def test_add_int(self) -> None:
        doc = Document.parse("")
        doc["count"] = 42
        assert doc["count"] == 42

    def test_add_float(self) -> None:
        doc = Document.parse("")
        doc["gravity"] = 9.81
        assert doc["gravity"] == 9.81

    def test_add_bool(self) -> None:
        doc = Document.parse("")
        doc["flag"] = True
        assert doc["flag"] == True  # noqa: E712

    def test_add_list(self) -> None:
        doc = Document.parse("")
        doc["items"] = [1, 2, 3]
        assert doc["items"][0] == 1
        assert doc["items"][2] == 3

    def test_add_dict(self) -> None:
        doc = Document.parse("")
        doc["meta"] = {"key": "value"}
        assert doc["meta"]["key"] == "value"

    def test_add_nested_dict(self) -> None:
        doc = Document.parse("")
        doc["a"] = {"b": {"c": "deep"}}
        assert doc["a"]["b"]["c"] == "deep"

    def test_add_list_of_dicts(self) -> None:
        doc = Document.parse("")
        doc["entries"] = [{"x": 1}, {"x": 2}]
        assert doc["entries"][0]["x"] == 1
        assert doc["entries"][1]["x"] == 2

    def test_add_mixed_list(self) -> None:
        doc = Document.parse("")
        doc["mix"] = [1, "two", 3.0]
        assert doc["mix"][0] == 1
        assert doc["mix"][1] == "two"
        assert doc["mix"][2] == 3.0

    def test_add_datetime(self) -> None:
        doc = Document.parse("")
        chicago = zoneinfo.ZoneInfo("America/Chicago")
        now = datetime(2026, 1, 15, 12, 30, 0, tzinfo=chicago)
        doc["ts"] = now
        assert "2026-01-15" in str(doc["ts"])


# ---------------------------------------------------------------------------
# Chained mutation on array-of-tables
# ---------------------------------------------------------------------------


class TestArrayOfTablesMutation:
    def test_set_element_to_scalar(self) -> None:
        doc = Document.parse("")
        doc["d"] = [{"a": 1}, {"b": 2}]
        doc["d"][0] = 7
        assert doc["d"][0] == 7

    def test_set_nested_value_in_table(self) -> None:
        doc = Document.parse("")
        doc["d"] = [{"a": 1}, {"b": 2}]
        doc["d"][1]["b"] = 99
        assert doc["d"][1]["b"] == 99

    def test_setitem_replaces_entry(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        doc["items"][0] = {"name": "z"}
        assert doc["items"][0]["name"] == "z"

    def test_setitem_negative_index(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        doc["items"][-1] = {"name": "last"}
        assert doc["items"][1]["name"] == "last"


# ---------------------------------------------------------------------------
# Chained mutation on inline tables (dicts)
# ---------------------------------------------------------------------------


class TestInlineTableMutation:
    def test_set_value_in_inline_table(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        doc["meta"]["x"] = 10
        assert doc["meta"]["x"] == 10

    def test_set_new_key_in_inline_table(self) -> None:
        doc = Document.parse("meta = {x = 1}\n")
        doc["meta"]["y"] = 2
        assert doc["meta"]["y"] == 2
        assert str(doc) == "meta = {x = 1, y = 2 }\n"

    def test_set_new_key_in_table(self, doc: Document) -> None:
        doc["owner"]["email"] = "alice@example.com"
        assert doc["owner"]["email"] == "alice@example.com"


# ---------------------------------------------------------------------------
# Format preservation
# ---------------------------------------------------------------------------


class TestFormatPreservation:
    def test_comments_preserved(self) -> None:
        toml = '# a comment\nkey = "value"\n'
        doc = Document.parse(toml)
        assert str(doc) == toml

    def test_whitespace_preserved(self) -> None:
        toml = 'key   =   "value"\n'
        doc = Document.parse(toml)
        assert str(doc) == toml

    def test_mutation_preserves_other_formatting(self) -> None:
        toml = "# header\na = 1\nb = 2\n"
        doc = Document.parse(toml)
        doc["a"] = 10
        assert str(doc) == "# header\na = 10\nb = 2\n"

    def test_inline_comment_preserved_on_top_level_update(self) -> None:
        toml = 'title = "old" # important note\n'
        doc = Document.parse(toml)
        doc["title"] = "new"
        assert str(doc) == 'title = "new" # important note\n'

    def test_inline_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\nname = "Tom"  # the owner name\nage = 30\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        assert str(doc) == '[owner]\nname = "Bob"  # the owner name\nage = 30\n'

    def test_standalone_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\n# this is the name\nname = "Tom"\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        assert str(doc) == '[owner]\n# this is the name\nname = "Bob"\n'


# ---------------------------------------------------------------------------
# Mutation via .get(), .items(), .values() (not just __getitem__)
# ---------------------------------------------------------------------------


class TestMutationViaAccessors:
    def test_get_returns_live_proxy(self, doc: Document) -> None:
        owner = doc.get("owner")
        assert owner is not None
        owner["name"] = "Bob"
        assert doc["owner"]["name"] == "Bob"

    def test_items_returns_live_proxies(self, doc: Document) -> None:
        for key, proxy in doc.items():
            if key == "owner":
                proxy["name"] = "Charlie"
                break
        assert doc["owner"]["name"] == "Charlie"

    def test_values_returns_live_proxies(self) -> None:
        doc = Document.parse("[section]\nval = 10\n")
        vals = doc.values()
        assert len(vals) == 1
        vals[0]["val"] = 99
        assert doc["section"]["val"] == 99


# ---------------------------------------------------------------------------
# Nested array access (navigate_parent with Key::Int)
# ---------------------------------------------------------------------------


class TestNestedArrayNavigation:
    """Exercise the Key::Int branch in navigate_parent / navigate_parent_mut."""

    def test_comment_on_nested_array_element(self) -> None:
        """doc["arr"][0] navigates to an array element; accessing a child
        of that element uses navigate_parent with int key in path."""
        doc = Document.parse("arr = [{x = 1}, {x = 2}]\n")
        assert doc["arr"][0]["x"] == 1
        assert doc["arr"][1]["x"] == 2

    def test_setitem_through_nested_array(self) -> None:
        doc = Document.parse("arr = [{x = 1}, {x = 2}]\n")
        doc["arr"][0]["x"] = 99
        assert doc["arr"][0]["x"] == 99

    def test_deeply_nested_array_access(self) -> None:
        doc = Document.parse("a = [[1, 2], [3, 4]]\n")
        assert doc["a"][0][0] == 1
        assert doc["a"][1][1] == 4


# ---------------------------------------------------------------------------
# Assigning a table (dict) to an existing key
# ---------------------------------------------------------------------------


class TestAssignTableToKey:
    def test_assign_dict_to_existing_scalar(self) -> None:
        """Assigning a dict to a key that was a scalar converts it to a table."""
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"]["x"] = {"nested": "value"}
        assert doc["section"]["x"]["nested"] == "value"

    def test_assign_dict_to_existing_table_key(self) -> None:
        """Assigning a dict to a key that was already a table replaces it."""
        doc = Document.parse("[s]\n[s.inner]\na = 1\n")
        doc["s"]["inner"] = {"b": 2}
        assert doc["s"]["inner"]["b"] == 2


# ---------------------------------------------------------------------------
# Assign Item (proxy) as a value
# ---------------------------------------------------------------------------


class TestAssignItemProxy:
    """Assigning an Item obtained from the document to another key."""

    def test_copy_array_to_new_key(self) -> None:
        doc = Document.parse("[project]\ndynamic = ['license', 'version']\n")
        doc["project"]["foo"] = doc["project"]["dynamic"]
        assert str(doc) == (
            "[project]\n"
            "dynamic = ['license', 'version']\n"
            "foo = ['license', 'version']\n"
        )

    def test_copy_scalar_to_new_key(self) -> None:
        doc = Document.parse("a = 1\n")
        doc["b"] = doc["a"]
        assert str(doc) == "a = 1\nb = 1\n"

    def test_copy_table_to_new_key(self) -> None:
        doc = Document.parse("[src]\nx = 1\ny = 2\n")
        doc["dst"] = doc["src"]
        assert doc["dst"]["x"] == 1
        assert doc["dst"]["y"] == 2

    def test_copy_is_independent(self) -> None:
        """After copying, changes to the original don't affect the copy."""
        doc = Document.parse("a = 1\n")
        doc["b"] = doc["a"]
        doc["a"] = 99
        assert doc["b"] == 1

    def test_proxy_setitem_on_nested_key(self) -> None:
        """doc["t"]["x"] = doc["t"]["y"] - proxy on both sides of nested setitem."""
        doc = Document.parse("[t]\nx = 1\ny = 2\n")
        doc["t"]["x"] = doc["t"]["y"]
        assert doc["t"]["x"] == 2

    def test_slice_assign_from_proxy(self) -> None:
        """Slice assignment with proxy values from the same document."""
        doc = Document.parse("a = [1, 2, 3]\nb = [10, 20]\n")
        doc["a"][0:1] = doc["b"]
        assert doc["a"] == [10, 20, 2, 3]

    def test_update_with_proxy_values(self) -> None:
        """update() where dict values are proxies into the same document."""
        doc = Document.parse("[t]\nx = 1\ny = 2\n")
        doc["t"].update({"z": doc["t"]["x"]})
        assert doc["t"]["z"] == 1

    def test_doc_update_with_proxy_values(self) -> None:
        """Document.update() where dict values are proxies into the same document."""
        doc = Document.parse("a = 1\nb = 2\n")
        doc.update({"c": doc["a"]})
        assert doc["c"] == 1

    def test_copy_from_another_document(self) -> None:
        """Proxies from a different document can be assigned too."""
        src = Document.parse("[t]\nfoo = 42\n")
        dst = Document.parse("[s]\nbar = 0\n")
        dst["s"]["bar"] = src["t"]["foo"]
        assert dst["s"]["bar"] == 42
        assert src["t"]["foo"] == 42

    def test_assign_document_as_table(self) -> None:
        """A whole Document can be assigned as a table under a key."""
        doc = Document.parse("x = 1\n")
        other = Document.parse("a = 10\nb = 20\n")
        doc["foo"] = other
        assert doc["foo"]["a"] == 10
        assert doc["foo"]["b"] == 20

    def test_assign_document_to_itself(self) -> None:
        """doc['foo'] = doc snapshots the current contents."""
        doc = Document.parse("a = 1\nb = 2\n")
        doc["foo"] = doc
        assert doc["foo"] == {"a": 1, "b": 2}
        assert "foo" not in doc["foo"]


# ---------------------------------------------------------------------------
# Item.parse() - custom TOML representations
# ---------------------------------------------------------------------------


class TestItemParse:
    def test_hex_integer(self) -> None:
        doc = Document.parse("x = 1\n")
        doc["x"] = Item.parse("0xFF")
        assert str(doc) == "x = 0xFF\n"
        assert doc["x"] == 255

    def test_literal_string(self) -> None:
        doc = Document.parse("x = 1\n")
        doc["x"] = Item.parse("'literal'")
        assert str(doc) == "x = 'literal'\n"

    def test_multiline_string(self) -> None:
        doc = Document.parse("x = 1\n")
        doc["x"] = Item.parse("'''multi\nline'''")
        assert str(doc) == "x = '''multi\nline'''\n"

    def test_value_is_correct(self) -> None:
        item = Item.parse("0xFF")
        assert item.value == 255

    def test_invalid_input_raises(self) -> None:
        with pytest.raises(ValueError, match="TOML parse error"):
            Item.parse("[not a value")
