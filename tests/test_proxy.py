"""Tests for Item proxy: len, iter, contains, bool, del, repr, str, value, fmt."""

from __future__ import annotations

from collections.abc import MutableMapping, MutableSequence
from datetime import date, datetime, time, timezone

import pytest

from tests.conftest import SAMPLE, toml_literal
from tomledit import DictItem, Document, Item, ListItem, ScalarItem

# ---------------------------------------------------------------------------
# Item: __len__
# ---------------------------------------------------------------------------


class TestProxyLen:
    def test_table_len(self, doc: Document) -> None:
        assert len(doc["owner"]) == 3  # name, age, active

    def test_array_len(self, doc: Document) -> None:
        assert len(doc["database"]["ports"]) == 3

    def test_empty_array_len(self) -> None:
        doc = Document.parse("arr = []\n")
        assert len(doc["arr"]) == 0

    def test_inline_table_len(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert len(doc["meta"]) == 2

    def test_scalar_len_raises(self, doc: Document) -> None:
        with pytest.raises(TypeError):
            len(doc["title"])

    def test_aot_len(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                [[items]]
                name = "b"
                [[items]]
                name = "c"
            """)
        )
        assert len(doc["items"]) == 3


# ---------------------------------------------------------------------------
# Item: __iter__
# ---------------------------------------------------------------------------


class TestProxyIter:
    def test_table_iter_yields_keys(self, doc: Document) -> None:
        keys = list(doc["owner"])
        assert set(keys) == {"name", "age", "active"}

    def test_inline_table_iter_yields_keys(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        keys = list(doc["meta"])
        assert set(keys) == {"x", "y"}

    def test_array_iter_yields_proxies(self, doc: Document) -> None:
        elems = list(doc["database"]["ports"])
        assert len(elems) == 3
        assert elems[0] == 8001
        assert elems[2] == 8002

    def test_array_iter_count(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        total = 0
        for _item in doc["arr"]:
            total += 1
        assert total == 3

    def test_empty_array_iter(self) -> None:
        doc = Document.parse("arr = []\n")
        assert list(doc["arr"]) == []

    def test_scalar_iter_raises(self, doc: Document) -> None:
        with pytest.raises(TypeError):
            iter(doc["title"])

    def test_aot_iter(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                [[items]]
                name = "b"
                [[items]]
                name = "c"
            """)
        )
        names = [entry["name"].value for entry in doc["items"]]
        assert names == ["a", "b", "c"]


# ---------------------------------------------------------------------------
# Item: __contains__
# ---------------------------------------------------------------------------


class TestProxyContains:
    def test_table_contains_key(self, doc: Document) -> None:
        assert "name" in doc["owner"]
        assert "email" not in doc["owner"]

    def test_inline_table_contains_key(self) -> None:
        doc = Document.parse("meta = {x = 1}\n")
        assert "x" in doc["meta"]
        assert "z" not in doc["meta"]

    def test_array_contains_value(self, doc: Document) -> None:
        assert 8001 in doc["database"]["ports"]
        assert 9999 not in doc["database"]["ports"]

    def test_array_contains_string(self) -> None:
        doc = Document.parse('arr = ["a", "b", "c"]\n')
        assert "b" in doc["arr"]
        assert "z" not in doc["arr"]

    def test_array_of_tables_contains_matching_dict(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        assert {"name": "a"} in doc["items"]
        assert {"name": "b"} in doc["items"]
        assert {"name": "c"} not in doc["items"]

    def test_array_of_tables_not_contains_partial(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            x = 1
            y = 2
        """)
        )
        assert {"x": 1} not in doc["items"]

    def test_array_of_tables_not_contains_non_dict(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        assert "not a dict" not in doc["items"]

    def test_scalar_string_contains(self) -> None:
        doc = Document.parse('msg = "hello world"\n')
        assert "world" in doc["msg"]
        assert "xyz" not in doc["msg"]

    def test_scalar_contains_non_string_raises(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            assert 1 in doc["val"]


# ---------------------------------------------------------------------------
# Item: __bool__
# ---------------------------------------------------------------------------


class TestProxyBool:
    def test_empty_array_falsy(self) -> None:
        doc = Document.parse("arr = []\n")
        assert bool(doc["arr"]) is False

    def test_nonempty_array_truthy(self, doc: Document) -> None:
        assert bool(doc["database"]["ports"]) is True

    def test_zero_int_falsy(self) -> None:
        doc = Document.parse("x = 0\n")
        assert bool(doc["x"]) is False

    def test_empty_string_falsy(self) -> None:
        doc = Document.parse('x = ""\n')
        assert bool(doc["x"]) is False

    @pytest.mark.parametrize(
        ("toml", "expected"),
        [
            ("[t]\na = 1\n", True),  # nonempty table
            ("[empty]\n", False),  # empty table
            ('title = "Example"\n', True),  # nonempty string scalar
            ("meta = {}\n", False),  # empty inline table
            ("x = 42\n", True),  # nonzero int
            ("x = 0.0\n", False),  # zero float
            ("x = 3.14\n", True),  # nonzero float
            ("x = false\n", False),  # false bool
            ("x = true\n", True),  # true bool
            ('x = "hello"\n', True),  # nonempty string
        ],
        ids=[
            "nonempty-table",
            "empty-table",
            "scalar-string",
            "empty-inline-table",
            "nonzero-int",
            "zero-float",
            "nonzero-float",
            "false-bool",
            "true-bool",
            "nonempty-string",
        ],
    )
    def test_bool_parametrized(self, toml: str, *, expected: bool) -> None:
        doc = Document.parse(toml)
        # Get the first (and only) key's value
        key = next(iter(doc))
        assert bool(doc[key]) is expected

    def test_aot_bool(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        assert bool(doc["items"]) is True

    def test_date_truthy(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert bool(doc["d"]) is True


# ---------------------------------------------------------------------------
# Item: __delitem__
# ---------------------------------------------------------------------------


class TestProxyDelitem:
    def test_del_table_key(self, doc: Document) -> None:
        del doc["owner"]["active"]
        assert "active" not in doc["owner"]
        assert len(doc["owner"]) == 2

    def test_del_array_element(self, doc: Document) -> None:
        del doc["database"]["ports"][0]
        assert len(doc["database"]["ports"]) == 2

    def test_del_inline_table_key(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        del doc["meta"]["x"]
        assert "x" not in doc["meta"]

    def test_del_missing_table_key_raises(self, doc: Document) -> None:
        with pytest.raises(KeyError):
            del doc["owner"]["nonexistent"]

    def test_del_array_out_of_bounds_raises(self, doc: Document) -> None:
        with pytest.raises(IndexError):
            del doc["database"]["ports"][99]

    def test_del_aot_first(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        del doc["items"][0]
        assert len(doc["items"]) == 1
        assert doc["items"][0]["name"] == "b"


# ---------------------------------------------------------------------------
# Item: __repr__
# ---------------------------------------------------------------------------


class TestProxyRepr:
    def test_repr_includes_type(self, doc: Document) -> None:
        r = repr(doc["owner"])
        assert "Item" in r

    def test_repr_includes_content(self, doc: Document) -> None:
        r = repr(doc["title"])
        assert "Example" in r

    def test_repr_scalar(self, doc: Document) -> None:
        r = repr(doc["owner"]["age"])
        assert "30" in r

    def test_aot_repr(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        r = repr(doc["items"])
        assert "Item(" in r


# ---------------------------------------------------------------------------
# Item: __str__
# ---------------------------------------------------------------------------


class TestStr:
    def test_document_roundtrip(self) -> None:
        doc = Document.parse(SAMPLE)
        assert doc.as_toml() == SAMPLE

    def test_document_str_is_dict_like(self) -> None:
        doc = Document.parse("x = 1\n")
        assert str(doc) == "{'x': 1}"

    def test_proxy_str_scalar(self) -> None:
        doc = Document.parse("x = 42\n")
        assert str(doc["x"]) == "42"

    def test_proxy_str_string(self) -> None:
        doc = Document.parse('name = "hello"\n')
        assert str(doc["name"]) == "hello"

    def test_proxy_str_int(self, doc: Document) -> None:
        assert str(doc["owner"]["age"]) == "30"

    def test_proxy_str_after_mutation(self, doc: Document) -> None:
        doc["owner"]["age"] = 99
        assert str(doc["owner"]["age"]) == "99"

    def test_proxy_str_float(self) -> None:
        doc = Document.parse("x = 3.14\n")
        assert str(doc["x"]) == "3.14"

    def test_proxy_str_float_whole(self) -> None:
        doc = Document.parse("x = 1.0\n")
        assert str(doc["x"]) == "1.0"

    def test_proxy_str_bool(self) -> None:
        doc = Document.parse("x = true\n")
        assert str(doc["x"]) == "True"


# ---------------------------------------------------------------------------
# as_toml
# ---------------------------------------------------------------------------


class TestAsToml:
    def test_document_as_toml(self) -> None:
        text = toml_literal("""
            name = "hello"
            age = 42
        """)
        doc = Document.parse(text)
        assert doc.as_toml() == text

    def test_bool_as_toml(self) -> None:
        doc = Document.parse("x = true\n")
        assert doc["x"].as_toml() == "true"

    def test_table_as_toml(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        assert doc["t"].as_toml() == "a = 1\nb = 2"

    def test_array_as_toml(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"].as_toml() == "[1, 2, 3]"

    def test_inline_table_as_toml(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert doc["meta"].as_toml() == "{x = 1, y = 2}"


# ---------------------------------------------------------------------------
# .value property
# ---------------------------------------------------------------------------


class TestValueProperty:
    """The .value property returns the native Python equivalent."""

    TOML = """\
count = 42
gravity = 9.81
name = "hello"
flag = true
arr = [1, 2, 3]
inline = {a = 1, b = 2}

[tbl]
x = 1
y = 2
"""

    def test_integer(self) -> None:
        doc = Document.parse(self.TOML)
        v = doc["count"].value
        assert v == 42
        assert type(v) is int

    def test_float(self) -> None:
        doc = Document.parse(self.TOML)
        v = doc["gravity"].value
        assert v == 9.81
        assert type(v) is float

    def test_string(self) -> None:
        doc = Document.parse(self.TOML)
        v = doc["name"].value
        assert v == "hello"
        assert type(v) is str

    def test_bool(self) -> None:
        doc = Document.parse(self.TOML)
        v = doc["flag"].value
        assert v is True

    def test_array(self) -> None:
        doc = Document.parse(self.TOML)
        v = doc["arr"].value
        assert v == [1, 2, 3]
        assert type(v) is list

    def test_inline_table(self) -> None:
        doc = Document.parse(self.TOML)
        v = doc["inline"].value
        assert v == {"a": 1, "b": 2}
        assert type(v) is dict

    def test_nested_table(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [a]
            [a.b]
            x = 1
        """)
        )
        v = doc["a"].value
        assert v == {"b": {"x": 1}}

    def test_augmented_assignment(self) -> None:
        """The motivating use case: doc['count'] += 4."""
        doc = Document.parse("count = 2\n")
        doc["count"] = doc["count"].value + 4
        assert doc["count"] == 6

    def test_value_is_a_copy(self) -> None:
        """Mutating .value does not affect the document."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        v = doc["arr"].value
        v.append(4)
        assert doc["arr"] == [1, 2, 3]

    def test_datetime(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        v = doc["dt"].value
        assert v == datetime(2024, 1, 15, 10, 30, tzinfo=timezone.utc)
        assert type(v) is datetime

    def test_date(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        v = doc["d"].value
        assert v == date(2024, 1, 15)
        assert type(v) is date

    def test_time(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        v = doc["t"].value
        assert v == time(10, 30)
        assert type(v) is time

    def test_aot_value(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                value = 1
                [[items]]
                name = "b"
                value = 2
            """)
        )
        v = doc["items"].value
        assert isinstance(v, list)
        assert len(v) == 2
        assert v[0] == {"name": "a", "value": 1}

    def test_datetime_with_offset(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00+05:30\n")
        v = doc["dt"].value
        assert isinstance(v, datetime)
        offset = v.utcoffset()
        assert offset is not None
        assert offset.total_seconds() == 5 * 3600 + 30 * 60

    def test_datetime_naive(self) -> None:
        """A datetime without timezone info."""
        doc = Document.parse("dt = 2024-01-15T10:30:00\n")
        v = doc["dt"].value
        assert isinstance(v, datetime)
        assert v.tzinfo is None
        assert v.hour == 10
        assert v.minute == 30


# ---------------------------------------------------------------------------
# ArrayOfTables: index access
# ---------------------------------------------------------------------------


class TestArrayOfTablesAccess:
    def test_getitem_int(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                [[items]]
                name = "b"
                [[items]]
                name = "c"
            """)
        )
        assert doc["items"][0]["name"] == "a"
        assert doc["items"][2]["name"] == "c"

    def test_getitem_out_of_range(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        with pytest.raises(IndexError):
            doc["items"][99]

    def test_aot_str(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        assert str(doc["items"]) == "[{'name': 'a'}]"


# ---------------------------------------------------------------------------
# Item: fmt
# ---------------------------------------------------------------------------


class TestProxyFmt:
    def test_fmt_table(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a   =   1
            b   =   2
        """)
        )
        doc["t"].fmt()
        assert doc.as_toml() == toml_literal("""
            [t]
            a = 1
            b = 2
        """)

    def test_fmt_inline_table(self) -> None:
        doc = Document.parse("meta = {x = 1 }\n")
        doc["meta"]["y"] = 2
        doc["meta"].fmt()
        assert doc.as_toml() == "meta = { x = 1, y = 2 }\n"

    def test_fmt_array(self) -> None:
        doc = Document.parse("arr = [  1,  2,  3  ]\n")
        doc["arr"].fmt()
        assert doc.as_toml() == "arr = [1, 2, 3]\n"

    def test_fmt_array_of_tables_is_noop(self) -> None:
        text = toml_literal("""
            [[t]]
            a   =   1
            [[t]]
            b   =   2
        """)
        doc = Document.parse(text)
        doc["t"].fmt()
        assert doc.as_toml() == text

    def test_fmt_scalar_is_noop(self) -> None:
        doc = Document.parse("x = 1\n")
        doc["x"].fmt()
        assert doc.as_toml() == "x = 1\n"

    def test_fmt_does_not_invalidate_proxies(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        t = doc["t"]
        b = doc["t"]["b"]
        t.fmt()
        assert b.value == 2


# ---------------------------------------------------------------------------
# Subclass types (DictItem / ListItem / ScalarItem)
# ---------------------------------------------------------------------------


class TestSubclassTypes:
    """__getitem__ returns the correct subclass."""

    def test_table_returns_dict_item(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [server]
            host = 'localhost'
        """)
        )
        assert type(doc["server"]).__name__ == "DictItem"

    def test_array_returns_list_item(self) -> None:
        doc = Document.parse("ports = [80, 443]")
        assert type(doc["ports"]).__name__ == "ListItem"

    def test_scalar_returns_scalar_item(self) -> None:
        doc = Document.parse("name = 'hello'")
        assert type(doc["name"]).__name__ == "ScalarItem"

    def test_nested_table(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [a]
            [a.b]
            c = 1
        """)
        )
        assert type(doc["a"]["b"]).__name__ == "DictItem"

    def test_nested_scalar(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [a]
            b = 42
        """)
        )
        assert type(doc["a"]["b"]).__name__ == "ScalarItem"

    def test_get_returns_typed(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [server]
            host = 'localhost'
        """)
        )
        assert type(doc["server"].get("host")).__name__ == "ScalarItem"

    def test_iteration_returns_typed(self) -> None:
        doc = Document.parse("ports = [80, 443]")
        for item in doc["ports"]:
            assert type(item).__name__ == "ScalarItem"

    def test_values_view_returns_typed(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [server]
            host = 'localhost'
            port = 8080
        """)
        )
        for v in doc["server"].values():
            assert isinstance(v, Item)
            assert type(v).__name__ == "ScalarItem"

    def test_items_view_returns_typed(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [server]
            host = 'localhost'
            port = 8080
        """)
        )
        for k, v in doc["server"].items():
            assert isinstance(k, str)
            assert isinstance(v, Item)

    def test_setitem_then_get_typed(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [server]
            host = 'localhost'
        """)
        )
        doc["server"]["port"] = 8080
        assert type(doc["server"]["port"]).__name__ == "ScalarItem"

    def test_slice_returns_typed(self) -> None:
        doc = Document.parse("ports = [80, 443, 8080]")
        for item in doc["ports"][0:2]:
            assert type(item).__name__ == "ScalarItem"

    def test_parse_returns_typed(self) -> None:
        assert type(Item.parse("0xFF")).__name__ == "ScalarItem"

    def test_dict_item_parse_inline_table(self) -> None:
        item = DictItem.parse("{a = 1}")
        assert isinstance(item, DictItem)
        assert item.value == {"a": 1}

    def test_list_item_parse_array(self) -> None:
        item = ListItem.parse("[1, 2]")
        assert isinstance(item, ListItem)
        assert item.value == [1, 2]

    def test_scalar_item_parse_int(self) -> None:
        item = ScalarItem.parse("42")
        assert isinstance(item, ScalarItem)
        assert item.value == 42

    def test_scalar_item_parse_string(self) -> None:
        item = ScalarItem.parse("'hello'")
        assert isinstance(item, ScalarItem)
        assert item.value == "hello"

    def test_dict_item_parse_rejects_array(self) -> None:
        with pytest.raises(ValueError, match=r"DictItem\.parse.*table.*ListItem"):
            DictItem.parse("[]")

    def test_dict_item_parse_rejects_scalar(self) -> None:
        with pytest.raises(ValueError, match=r"DictItem\.parse.*table.*ScalarItem"):
            DictItem.parse("42")

    def test_list_item_parse_rejects_table(self) -> None:
        with pytest.raises(ValueError, match=r"ListItem\.parse.*array.*DictItem"):
            ListItem.parse("{}")

    def test_list_item_parse_rejects_scalar(self) -> None:
        with pytest.raises(ValueError, match=r"ListItem\.parse.*array.*ScalarItem"):
            ListItem.parse("42")

    def test_scalar_item_parse_rejects_array(self) -> None:
        with pytest.raises(ValueError, match=r"ScalarItem\.parse.*scalar.*ListItem"):
            ScalarItem.parse("[]")

    def test_scalar_item_parse_rejects_table(self) -> None:
        with pytest.raises(ValueError, match=r"ScalarItem\.parse.*scalar.*DictItem"):
            ScalarItem.parse("{}")


class TestIsinstance:
    """ABC registration gives isinstance() support."""

    def test_dict_item_is_mutable_mapping(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [server]
            host = 'localhost'
        """)
        )
        assert isinstance(doc["server"], MutableMapping)

    def test_list_item_is_mutable_sequence(self) -> None:
        doc = Document.parse("ports = [80, 443]")
        assert isinstance(doc["ports"], MutableSequence)

    def test_scalar_is_not_mapping_or_sequence(self) -> None:
        doc = Document.parse("name = 'hello'")
        assert not isinstance(doc["name"], MutableMapping)
        assert not isinstance(doc["name"], MutableSequence)

    def test_all_are_item(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [server]
            host = 'localhost'
            ports = [80]
        """)
        )
        assert isinstance(doc["server"], Item)
        assert isinstance(doc["server"]["ports"], Item)
        assert isinstance(doc["server"]["host"], Item)
