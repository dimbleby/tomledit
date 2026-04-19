"""Tests for writing/mutating values, keys, arrays, and tables."""

from __future__ import annotations

import zoneinfo
from datetime import date, datetime, time

import pytest

from tests.conftest import ItemsMapping, toml_literal
from tomledit import Document, Item

# ---------------------------------------------------------------------------
# Navigation (reading deeply nested paths and array indices)
# ---------------------------------------------------------------------------


class TestNavigation:
    def test_deeply_nested(self, doc: Document) -> None:
        assert doc["servers"]["alpha"]["ip"] == "10.0.0.1"
        assert doc["servers"]["beta"]["role"] == "backend"

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
        assert 'name = "Bob"' in doc.as_toml()
        assert doc["owner"]["name"] == "Bob"


# ---------------------------------------------------------------------------
# Writing array elements (like list assignment)
# ---------------------------------------------------------------------------


class TestWriteArrayElements:
    def test_replace_array_element(self, doc: Document) -> None:
        doc["database"]["ports"][0] = 9999
        assert doc["database"]["ports"][0] == 9999


# ---------------------------------------------------------------------------
# Setting new keys (like dict[key] = value)
# ---------------------------------------------------------------------------


class TestSetNewKeys:
    def test_add_string(self) -> None:
        doc = Document()
        doc["name"] = "hello"
        assert doc["name"] == "hello"

    def test_add_int(self) -> None:
        doc = Document()
        doc["count"] = 42
        assert doc["count"] == 42

    def test_add_float(self) -> None:
        doc = Document()
        doc["gravity"] = 9.81
        assert doc["gravity"] == 9.81

    def test_add_bool(self) -> None:
        doc = Document()
        doc["flag"] = True
        assert doc["flag"] == True  # noqa: E712

    def test_add_list(self) -> None:
        doc = Document()
        doc["items"] = [1, 2, 3]
        assert doc["items"][0] == 1
        assert doc["items"][2] == 3

    def test_add_dict(self) -> None:
        doc = Document()
        doc["meta"] = {"key": "value"}
        assert doc["meta"]["key"] == "value"

    def test_add_dict_with_proxy_key(self) -> None:
        base = Document.parse('key = "name"\n')
        doc = Document()
        doc["meta"] = {base["key"]: "value"}
        assert doc["meta"]["name"] == "value"

    def test_add_mapping_with_list_pairs(self) -> None:
        doc = Document()
        doc["meta"] = ItemsMapping({"key": "value"}, [["key", "value"]])
        assert doc["meta"]["key"] == "value"

    def test_add_inline_table_mapping_with_list_pairs(self) -> None:
        doc = Document.parse("dst = { a = 1 }\n")
        doc["dst"]["child"] = ItemsMapping({"b": 2}, [["b", 2]])
        assert doc.as_toml() == "dst = { a = 1, child = { b = 2 } }\n"

    def test_add_inline_table_dict_with_proxy_key(self) -> None:
        base = Document.parse('key = "b"\n')
        doc = Document.parse("dst = { a = 1 }\n")
        doc["dst"]["child"] = {base["key"]: 2}
        assert doc.as_toml() == "dst = { a = 1, child = { b = 2 } }\n"

    @pytest.mark.parametrize(
        "pair",
        [[], ["x"], ["x", 1, 2]],
        ids=["empty", "short", "long"],
    )
    def test_add_mapping_rejects_non_pair_items(self, pair: list[object]) -> None:
        doc = Document()
        with pytest.raises(ValueError, match="expected a length-2 iterable pair"):
            doc["meta"] = ItemsMapping({"x": 1}, [pair])

    def test_add_mapping_rejects_non_string_pair_key(self) -> None:
        doc = Document()
        with pytest.raises(TypeError, match="keys must be strings"):
            doc["meta"] = ItemsMapping({"unused": 0}, [[1, "value"]])

    def test_add_list_of_dicts(self) -> None:
        doc = Document()
        doc["entries"] = [{"x": 1}, {"x": 2}]
        assert doc["entries"][0]["x"] == 1
        assert doc["entries"][1]["x"] == 2

    def test_add_mixed_list(self) -> None:
        doc = Document()
        doc["mix"] = [1, "two", 3.0]
        assert doc["mix"][0] == 1
        assert doc["mix"][1] == "two"
        assert doc["mix"][2] == 3.0

    def test_add_mixed_list_with_inline_tables(self) -> None:
        doc = Document()
        doc["mix"] = [1, {"a": 2}, "three"]
        assert doc["mix"][0] == 1
        assert doc["mix"][1]["a"] == 2
        assert doc["mix"][2] == "three"
        assert doc.as_toml() == 'mix = [1, { a = 2 }, "three"]\n'

    def test_add_datetime(self) -> None:
        doc = Document()
        chicago = zoneinfo.ZoneInfo("America/Chicago")
        now = datetime(2026, 1, 15, 12, 30, 0, tzinfo=chicago)
        doc["ts"] = now
        assert "2026-01-15" in str(doc["ts"])

    def test_add_date(self) -> None:
        doc = Document()
        value = date(2024, 1, 15)
        doc["d"] = value
        assert doc["d"].value == value
        assert doc.as_toml() == "d = 2024-01-15\n"

    def test_add_time(self) -> None:
        doc = Document()
        value = time(10, 30, 45)
        doc["t"] = value
        assert doc["t"].value == value
        assert doc.as_toml() == "t = 10:30:45\n"

    def test_add_time_with_tzinfo_raises(self) -> None:
        doc = Document()
        utc = zoneinfo.ZoneInfo("UTC")
        value = time(10, 30, 45, tzinfo=utc)
        with pytest.raises(TypeError, match="TOML local times"):
            doc["t"] = value

    def test_add_datetime_zero_microseconds(self) -> None:
        doc = Document()
        value = datetime(2024, 1, 15, 12, 0, 0, 0)  # noqa: DTZ001
        doc["ts"] = value
        assert doc.as_toml() == "ts = 2024-01-15T12:00:00\n"


# ---------------------------------------------------------------------------
# Chained mutation on array-of-tables
# ---------------------------------------------------------------------------


class TestArrayOfTablesMutation:
    def test_set_element_to_scalar_raises(self) -> None:
        doc = Document()
        doc["d"] = [{"a": 1}, {"b": 2}]
        with pytest.raises(TypeError, match="expected a table"):
            doc["d"][0] = 7

    def test_set_nested_value_in_table(self) -> None:
        doc = Document()
        doc["d"] = [{"a": 1}, {"b": 2}]
        doc["d"][1]["b"] = 99
        assert doc["d"][1]["b"] == 99

    def test_setitem_negative_index(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"][-1] = {"name": "last"}
        assert doc["items"][1]["name"] == "last"

    def test_setitem_inline_table_proxy(self) -> None:
        """Assigning an inline-table proxy to an AoT index should produce valid TOML."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        source = Document.parse('entry = {name = "replaced"}\n')
        doc["items"][0] = source["entry"]
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "replaced"
            [[items]]
            name = "b"
        """)


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
        assert doc.as_toml() == "meta = {x = 1, y = 2 }\n"

    def test_set_new_key_in_table(self, doc: Document) -> None:
        doc["owner"]["email"] = "alice@example.com"
        assert doc["owner"]["email"] == "alice@example.com"

    def test_assign_dict_to_inline_table_key(self) -> None:
        doc = Document.parse("it = { a = 1 }\n")
        doc["it"]["b"] = {"foo": "bar"}
        assert doc["it"]["b"]["foo"] == "bar"
        assert list(doc["it"]) == ["a", "b"]
        assert doc.as_toml() == 'it = { a = 1, b = { foo = "bar" } }\n'
        doc2 = Document.parse(doc.as_toml())
        assert doc2 == doc


# ---------------------------------------------------------------------------
# Nested array access (navigate_parent with Key::Int)
# ---------------------------------------------------------------------------


class TestNestedArrayNavigation:
    """Exercise the Key::Int branch in navigate_parent / navigate_parent_mut."""

    def test_read_value_from_nested_array_element(self) -> None:
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
        doc = Document.parse(
            toml_literal("""
            [section]
            x = 1
        """)
        )
        doc["section"]["x"] = {"nested": "value"}
        assert doc["section"]["x"]["nested"] == "value"

    def test_assign_dict_to_existing_table_key(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [s]
            [s.inner]
            a = 1
        """)
        )
        doc["s"]["inner"] = {"b": 2}
        assert doc["s"]["inner"]["b"] == 2

    def test_assign_empty_dict_creates_standard_table(self) -> None:
        doc = Document()
        doc["foo"] = {}
        assert doc.as_toml() == "[foo]\n"

    def test_assign_dict_creates_standard_table(self) -> None:
        doc = Document()
        doc["foo"] = {"bar": 1, "baz": "hello"}
        result = doc.as_toml()
        assert "[foo]" in result
        assert "bar = 1" in result
        assert 'baz = "hello"' in result

    def test_assign_nested_dict_creates_dotted_table(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [existing]
            key = 1
        """)
        )
        doc["existing"]["nested"] = {"a": 2}
        result = doc.as_toml()
        assert "[existing.nested]" in result
        assert "a = 2" in result

    def test_existing_inline_table_stays_inline(self) -> None:
        doc = Document.parse("foo = { bar = 1 }\n")
        doc["foo"]["bar"] = 2
        assert doc.as_toml() == "foo = { bar = 2 }\n"

    def test_assign_list_of_dicts_creates_array_of_tables(self) -> None:
        doc = Document()
        doc["servers"] = [{"name": "alpha"}, {"name": "beta"}]
        result = doc.as_toml()
        assert result.count("[[servers]]") == 2
        assert 'name = "alpha"' in result
        assert 'name = "beta"' in result

    def test_assign_empty_list_creates_regular_array(self) -> None:
        doc = Document()
        doc["items"] = []
        assert doc.as_toml() == "items = []\n"

    def test_assign_nested_list_of_dicts_creates_dotted_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [project]
            name = "foo"
        """)
        )
        doc["project"]["authors"] = [{"name": "Alice"}, {"name": "Bob"}]
        result = doc.as_toml()
        assert result.count("[[project.authors]]") == 2
        assert 'name = "Alice"' in result
        assert 'name = "Bob"' in result

    def test_assign_deeply_nested_dict_creates_regular_tables(self) -> None:
        doc = Document()
        doc["foo"] = {"foo": {"bar": "baz"}}
        assert doc.as_toml() == toml_literal("""
            [foo]

            [foo.foo]
            bar = "baz"
        """)

    def test_assign_nested_aot_inside_dict(self) -> None:
        doc = Document()
        doc["pkg"] = {"servers": [{"name": "a"}, {"name": "b"}]}
        assert doc.as_toml() == toml_literal("""
            [pkg]

            [[pkg.servers]]
            name = "a"

            [[pkg.servers]]
            name = "b"
        """)


# ---------------------------------------------------------------------------
# Assign Item (proxy) as a value
# ---------------------------------------------------------------------------


class TestAssignItemProxy:
    """Assigning an Item obtained from the document to another key."""

    def test_copy_array_to_new_key(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [project]
            dynamic = ['license', 'version']
        """)
        )
        doc["project"]["foo"] = doc["project"]["dynamic"]
        assert doc.as_toml() == (
            "[project]\n"
            "dynamic = ['license', 'version']\n"
            "foo = ['license', 'version']\n"
        )

    def test_copy_scalar_to_new_key(self) -> None:
        doc = Document.parse("a = 1\n")
        doc["b"] = doc["a"]
        assert doc.as_toml() == toml_literal("""
            a = 1
            b = 1
        """)

    def test_copy_table_to_new_key(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [src]
            x = 1
            y = 2
        """)
        )
        doc["dst"] = doc["src"]
        assert doc["dst"]["x"] == 1
        assert doc["dst"]["y"] == 2

    def test_copy_table_from_another_doc_has_blank_line(self) -> None:
        src = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        dst = Document.parse(
            toml_literal("""
            [existing]
            key = 1
        """)
        )
        dst["t"] = src["t"]
        expected = toml_literal("""
            [existing]
            key = 1

            [t]
            a = 1
        """)
        assert dst.as_toml() == expected

    def test_copy_aot_from_another_doc_has_blank_line(self) -> None:
        src = Document.parse(
            toml_literal("""
            [[items]]
            x = 1

            [[items]]
            x = 2
        """)
        )
        dst = Document.parse(
            toml_literal("""
            [existing]
            key = 1
        """)
        )
        dst["items"] = src["items"]
        expected = toml_literal("""
            [existing]
            key = 1

            [[items]]
            x = 1

            [[items]]
            x = 2
        """)
        assert dst.as_toml() == expected

    def test_copy_is_independent(self) -> None:
        doc = Document.parse("a = 1\n")
        doc["b"] = doc["a"]
        doc["a"] = 99
        assert doc["b"] == 1

    def test_slice_assign_from_proxy(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = [1, 2, 3]
            b = [10, 20]
        """)
        )
        doc["a"][0:1] = doc["b"]
        assert doc["a"] == [10, 20, 2, 3]

    def test_update_with_proxy_values(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            x = 1
            y = 2
        """)
        )
        doc["t"].update({"z": doc["t"]["x"]})
        assert doc["t"]["z"] == 1

    def test_doc_update_with_proxy_values(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        doc.update({"c": doc["a"]})
        assert doc["c"] == 1

    def test_copy_from_another_document(self) -> None:
        src = Document.parse(
            toml_literal("""
            [t]
            foo = 42
        """)
        )
        dst = Document.parse(
            toml_literal("""
            [s]
            bar = 0
        """)
        )
        dst["s"]["bar"] = src["t"]["foo"]
        assert dst["s"]["bar"] == 42
        assert src["t"]["foo"] == 42

    def test_assign_document_as_table(self) -> None:
        doc = Document.parse("x = 1\n")
        other = Document.parse(
            toml_literal("""
            a = 10
            b = 20
        """)
        )
        doc["foo"] = other
        assert doc["foo"]["a"] == 10
        assert doc["foo"]["b"] == 20

    def test_assign_document_to_itself(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
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
        assert doc.as_toml() == "x = 0xFF\n"
        assert doc["x"] == 255

    def test_literal_string(self) -> None:
        doc = Document.parse("x = 1\n")
        doc["x"] = Item.parse("'literal'")
        assert doc.as_toml() == "x = 'literal'\n"

    def test_item_parse_value_returns_int(self) -> None:
        item = Item.parse("0xFF")
        assert item.value == 255

    def test_invalid_input_raises(self) -> None:
        with pytest.raises(ValueError, match="TOML parse error"):
            Item.parse("[not a value")

    def test_inline_table(self) -> None:
        item = Item.parse('{a = 1, b = "hi"}')
        assert item.value == {"a": 1, "b": "hi"}

    def test_array(self) -> None:
        item = Item.parse("[1, 2, 3]")
        assert item.value == [1, 2, 3]

    def test_date(self) -> None:
        item = Item.parse("2024-01-15")
        assert item.value == date(2024, 1, 15)

    def test_time(self) -> None:
        item = Item.parse("10:30:00")
        assert item.value == time(10, 30, 0)


# ---------------------------------------------------------------------------
# Tuple rejection
# ---------------------------------------------------------------------------


class TestTupleRejection:
    def test_tuple_assignment_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="tuple"):
            doc["x"] = (1, 2, 3)


# ---------------------------------------------------------------------------
# Format preservation on mutation
# ---------------------------------------------------------------------------


class TestFormatPreservation:
    def test_whitespace_preserved(self) -> None:
        toml = 'key   =   "value"\n'
        doc = Document.parse(toml)
        assert doc.as_toml() == toml

    def test_mutation_preserves_other_formatting(self) -> None:
        toml = toml_literal("""
            # header
            a = 1
            b = 2
        """)
        doc = Document.parse(toml)
        doc["a"] = 10
        assert doc.as_toml() == toml_literal("""
            # header
            a = 10
            b = 2
        """)

    def test_new_key_inherits_sibling_indent(self) -> None:
        """Adding a new key to an indented table copies the sibling indent."""
        doc = Document.parse(
            toml_literal("""
            [[fruit]]
              name = "apple"
        """)
        )
        doc["fruit"][0]["fresh"] = True
        assert doc.as_toml() == toml_literal("""
            [[fruit]]
              name = "apple"
              fresh = true
        """)

    def test_new_key_no_indent_when_siblings_unindented(self) -> None:
        """Default (unindented) tables stay unindented."""
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        doc["t"]["b"] = 2
        assert doc.as_toml() == toml_literal("""
            [t]
            a = 1
            b = 2
        """)

    def test_new_key_in_inline_table_no_indent(self) -> None:
        """Inline tables never gain multi-line indentation."""
        doc = Document.parse("t = {a = 1, b = 2}\n")
        doc["t"]["c"] = 3
        assert "\n" not in doc.as_toml().rstrip("\n")

    def test_new_key_no_indent_when_sibling_has_comment_prefix(self) -> None:
        """A sibling key whose prefix ends in a comment yields no indent."""
        doc = Document.parse(
            toml_literal("""
            [t]
            # leading comment
            a = 1
        """)
        )
        doc["t"]["b"] = 2
        assert doc.as_toml() == toml_literal("""
            [t]
            # leading comment
            a = 1
            b = 2
        """)
