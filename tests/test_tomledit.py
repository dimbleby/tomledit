"""Tests for tomledit: manipulating TOML documents like native Python objects."""

from __future__ import annotations

import zoneinfo
from datetime import date, datetime, time, timezone

import pytest

from tomledit import Document

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

SAMPLE = """\
title = "Example"

[owner]
name = "Alice"
age = 30
active = true

[database]
ports = [8001, 8001, 8002]
connection_max = 5000
enabled = true

[servers]

[servers.alpha]
ip = "10.0.0.1"
role = "frontend"

[servers.beta]
ip = "10.0.0.2"
role = "backend"
"""


def make_doc() -> Document:
    return Document.parse(SAMPLE)


# ---------------------------------------------------------------------------
# Reading scalar values (like dict access)
# ---------------------------------------------------------------------------


class TestReadScalars:
    def test_top_level_string(self) -> None:
        doc = make_doc()
        assert doc["title"] == "Example"

    def test_nested_string(self) -> None:
        doc = make_doc()
        assert doc["owner"]["name"] == "Alice"

    def test_nested_int(self) -> None:
        doc = make_doc()
        assert doc["owner"]["age"] == 30

    def test_nested_bool(self) -> None:
        doc = make_doc()
        assert doc["owner"]["active"] == True  # noqa: E712

    def test_deeply_nested(self) -> None:
        doc = make_doc()
        assert doc["servers"]["alpha"]["ip"] == "10.0.0.1"
        assert doc["servers"]["beta"]["role"] == "backend"


# ---------------------------------------------------------------------------
# Reading array elements (like list access)
# ---------------------------------------------------------------------------


class TestReadArrays:
    def test_array_element_by_index(self) -> None:
        doc = make_doc()
        assert doc["database"]["ports"][0] == 8001
        assert doc["database"]["ports"][2] == 8002

    def test_array_out_of_bounds(self) -> None:
        doc = make_doc()
        with pytest.raises(IndexError):
            doc["database"]["ports"][99]


# ---------------------------------------------------------------------------
# Writing scalar values
# ---------------------------------------------------------------------------


class TestWriteScalars:
    def test_set_top_level_string(self) -> None:
        doc = make_doc()
        doc["title"] = "Changed"
        assert doc["title"] == "Changed"

    def test_set_nested_int(self) -> None:
        doc = make_doc()
        doc["owner"]["age"] = 31
        assert doc["owner"]["age"] == 31

    def test_set_nested_bool(self) -> None:
        doc = make_doc()
        doc["owner"]["active"] = False
        assert doc["owner"]["active"] == False  # noqa: E712

    def test_set_nested_float(self) -> None:
        doc = make_doc()
        doc["database"]["connection_max"] = 9.81
        assert doc["database"]["connection_max"] == 9.81

    def test_set_deeply_nested_string(self) -> None:
        doc = make_doc()
        doc["servers"]["alpha"]["ip"] = "192.168.1.1"
        assert doc["servers"]["alpha"]["ip"] == "192.168.1.1"

    def test_mutation_persists_in_str(self) -> None:
        doc = make_doc()
        doc["owner"]["name"] = "Bob"
        assert 'name = "Bob"' in str(doc)


# ---------------------------------------------------------------------------
# Writing array elements (like list assignment)
# ---------------------------------------------------------------------------


class TestWriteArrayElements:
    def test_replace_array_element(self) -> None:
        doc = make_doc()
        doc["database"]["ports"][0] = 9999
        assert doc["database"]["ports"][0] == 9999

    def test_replace_does_not_affect_others(self) -> None:
        doc = make_doc()
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


# ---------------------------------------------------------------------------
# Chained mutation on inline tables (dicts)
# ---------------------------------------------------------------------------


class TestInlineTableMutation:
    def test_set_value_in_inline_table(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        doc["meta"]["x"] = 10
        assert doc["meta"]["x"] == 10

    def test_set_new_key_in_table(self) -> None:
        doc = make_doc()
        doc["owner"]["email"] = "alice@example.com"
        assert doc["owner"]["email"] == "alice@example.com"


# ---------------------------------------------------------------------------
# Dict-like protocol on Document
# ---------------------------------------------------------------------------


class TestDocumentDictProtocol:
    def test_contains(self) -> None:
        doc = make_doc()
        assert "title" in doc
        assert "nonexistent" not in doc

    def test_len(self) -> None:
        doc = make_doc()
        assert len(doc) == 4  # title, owner, database, servers

    def test_iter(self) -> None:
        doc = make_doc()
        # __iter__ currently returns a list of keys
        keys = list(doc)
        assert "title" in keys
        assert "owner" in keys

    def test_keys(self) -> None:
        doc = make_doc()
        assert set(doc.keys()) == {"title", "owner", "database", "servers"}

    def test_del(self) -> None:
        doc = make_doc()
        del doc["title"]
        assert "title" not in doc

    def test_pop(self) -> None:
        doc = make_doc()
        doc.pop("title")
        assert "title" not in doc

    def test_clear(self) -> None:
        doc = make_doc()
        doc.clear()
        assert len(doc) == 0

    def test_get_existing(self) -> None:
        doc = make_doc()
        item = doc.get("title")
        assert item is not None

    def test_get_missing(self) -> None:
        doc = make_doc()
        assert doc.get("nope") is None

    def test_getitem_missing_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(KeyError):
            doc["nope"]


# ---------------------------------------------------------------------------
# String representation (__str__)
# ---------------------------------------------------------------------------


class TestStr:
    def test_document_roundtrip(self) -> None:
        doc = Document.parse(SAMPLE)
        assert str(doc) == SAMPLE

    def test_proxy_str_scalar(self) -> None:
        doc = Document.parse("x = 42\n")
        assert str(doc["x"]) == "42"

    def test_proxy_str_string(self) -> None:
        doc = Document.parse('name = "hello"\n')
        assert str(doc["name"]) == "hello"

    def test_proxy_str_int(self) -> None:
        doc = make_doc()
        assert str(doc["owner"]["age"]) == "30"

    def test_proxy_str_after_mutation(self) -> None:
        doc = make_doc()
        doc["owner"]["age"] = 99
        assert str(doc["owner"]["age"]) == "99"


# ---------------------------------------------------------------------------
# Equality semantics
# ---------------------------------------------------------------------------


class TestEquality:
    def test_int_eq(self) -> None:
        doc = make_doc()
        assert doc["owner"]["age"] == 30
        assert doc["owner"]["age"] != 31

    def test_str_eq(self) -> None:
        doc = make_doc()
        assert doc["owner"]["name"] == "Alice"
        assert doc["owner"]["name"] != "Bob"

    def test_bool_eq(self) -> None:
        doc = make_doc()
        assert doc["owner"]["active"] == True  # noqa: E712
        assert doc["owner"]["active"] != False  # noqa: E712

    def test_float_eq(self) -> None:
        doc = Document.parse("val = 2.5\n")
        assert doc["val"] == 2.5

    def test_type_mismatch_not_equal(self) -> None:
        doc = make_doc()
        assert doc["owner"]["age"] != "30"
        assert doc["owner"]["name"] != 42

    def test_datetime_proxy_eq(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        expected = datetime(2024, 1, 15, 10, 30, 0, tzinfo=timezone.utc)
        assert doc["dt"] == expected

    def test_datetime_ne(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        wrong = datetime(2000, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
        assert doc["dt"] != wrong

    def test_array_equals_list(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] == [1, 2, 3]

    def test_array_not_equals_different_list(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] != [1, 2, 4]

    def test_array_not_equals_different_length(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] != [1, 2]

    def test_empty_array_equals_empty_list(self) -> None:
        doc = Document.parse("arr = []\n")
        assert doc["arr"] == []

    def test_table_equals_dict(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        assert doc["t"] == {"a": 1, "b": 2}

    def test_table_not_equals_different_dict(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert doc["t"] != {"a": 2}

    def test_inline_table_equals_dict(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert doc["meta"] == {"x": 1, "y": 2}

    def test_nested_array_equals_nested_list(self) -> None:
        doc = Document.parse("arr = [[1, 2], [3, 4]]\n")
        assert doc["arr"] == [[1, 2], [3, 4]]

    def test_string_array_equals_list(self) -> None:
        doc = Document.parse('arr = ["a", "b"]\n')
        assert doc["arr"] == ["a", "b"]

    def test_reverse_equality(self) -> None:
        """Python falls back to proxy's __eq__ in both directions."""
        doc = make_doc()
        assert 30 == doc["owner"]["age"]  # noqa: SIM300
        assert [8001, 8001, 8002] == doc["database"]["ports"]  # noqa: SIM300

    def test_bool_not_equal_to_int(self) -> None:
        """TOML types are strict: bool != int, even though Python's True == 1."""
        doc = Document.parse("count = 1\nflag = true\n")
        assert doc["count"] != True  # noqa: E712
        assert doc["flag"] != 1

    def test_bool_not_equal_to_float(self) -> None:
        doc = Document.parse("val = 1.0\n")
        assert doc["val"] != True  # noqa: E712

    def test_proxy_vs_proxy_same_value(self) -> None:
        """Two proxies pointing at equal values should compare equal."""
        doc = Document.parse("a = 42\nb = 42\n")
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_different_value(self) -> None:
        doc = Document.parse("a = 42\nb = 99\n")
        assert doc["a"] != doc["b"]

    def test_proxy_self_equality(self) -> None:
        doc = Document.parse("a = 42\n")
        assert doc["a"] == doc["a"]

    def test_proxy_vs_proxy_nested(self) -> None:
        doc = Document.parse("[t1]\nx = 1\n\n[t2]\nx = 1\n")
        assert doc["t1"] == doc["t2"]

    def test_proxy_vs_proxy_type_strict(self) -> None:
        """Proxy-vs-proxy should preserve TOML type strictness."""
        doc = Document.parse("count = 1\nflag = true\n")
        assert doc["count"] != doc["flag"]


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
        result = str(doc)
        assert "# header" in result
        assert "b = 2" in result

    def test_inline_comment_preserved_on_top_level_update(self) -> None:
        toml = 'title = "old" # important note\n'
        doc = Document.parse(toml)
        doc["title"] = "new"
        assert "# important note" in str(doc)

    def test_inline_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\nname = "Tom"  # the owner name\nage = 30\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        result = str(doc)
        assert '"Bob"' in result
        assert "# the owner name" in result

    def test_standalone_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\n# this is the name\nname = "Tom"\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        result = str(doc)
        assert "# this is the name" in result
        assert '"Bob"' in result


# ---------------------------------------------------------------------------
# Mutation via .get(), .items(), .values() (not just __getitem__)
# ---------------------------------------------------------------------------


class TestMutationViaAccessors:
    def test_get_returns_live_proxy(self) -> None:
        doc = make_doc()
        owner = doc.get("owner")
        assert owner is not None
        owner["name"] = "Bob"
        assert doc["owner"]["name"] == "Bob"

    def test_items_returns_live_proxies(self) -> None:
        doc = make_doc()
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
# Item: __len__
# ---------------------------------------------------------------------------


class TestProxyLen:
    def test_table_len(self) -> None:
        doc = make_doc()
        assert len(doc["owner"]) == 3  # name, age, active

    def test_array_len(self) -> None:
        doc = make_doc()
        assert len(doc["database"]["ports"]) == 3

    def test_empty_array_len(self) -> None:
        doc = Document.parse("arr = []\n")
        assert len(doc["arr"]) == 0

    def test_inline_table_len(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert len(doc["meta"]) == 2

    def test_scalar_len_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(TypeError):
            len(doc["title"])


# ---------------------------------------------------------------------------
# Item: __iter__
# ---------------------------------------------------------------------------


class TestProxyIter:
    def test_table_iter_yields_keys(self) -> None:
        doc = make_doc()
        keys = list(doc["owner"])
        assert set(keys) == {"name", "age", "active"}

    def test_inline_table_iter_yields_keys(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        keys = list(doc["meta"])
        assert set(keys) == {"x", "y"}

    def test_array_iter_yields_proxies(self) -> None:
        doc = make_doc()
        elems = list(doc["database"]["ports"])
        assert len(elems) == 3
        assert elems[0] == 8001
        assert elems[2] == 8002

    def test_array_iter_for_loop(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        total = 0
        for _item in doc["arr"]:
            total += 1
        assert total == 3

    def test_empty_array_iter(self) -> None:
        doc = Document.parse("arr = []\n")
        assert list(doc["arr"]) == []

    def test_scalar_iter_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(TypeError):
            iter(doc["title"])


# ---------------------------------------------------------------------------
# Item: __contains__
# ---------------------------------------------------------------------------


class TestProxyContains:
    def test_table_contains_key(self) -> None:
        doc = make_doc()
        assert "name" in doc["owner"]
        assert "email" not in doc["owner"]

    def test_inline_table_contains_key(self) -> None:
        doc = Document.parse("meta = {x = 1}\n")
        assert "x" in doc["meta"]
        assert "z" not in doc["meta"]

    def test_array_contains_value(self) -> None:
        doc = make_doc()
        assert 8001 in doc["database"]["ports"]
        assert 9999 not in doc["database"]["ports"]

    def test_array_contains_string(self) -> None:
        doc = Document.parse('arr = ["a", "b", "c"]\n')
        assert "b" in doc["arr"]
        assert "z" not in doc["arr"]

    def test_array_of_tables_contains_matching_dict(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        assert {"name": "a"} in doc["items"]
        assert {"name": "b"} in doc["items"]
        assert {"name": "c"} not in doc["items"]

    def test_array_of_tables_not_contains_partial(self) -> None:
        doc = Document.parse("[[items]]\nx = 1\ny = 2\n")
        assert {"x": 1} not in doc["items"]


# ---------------------------------------------------------------------------
# Item: __bool__
# ---------------------------------------------------------------------------


class TestProxyBool:
    def test_nonempty_array_truthy(self) -> None:
        doc = make_doc()
        assert bool(doc["database"]["ports"]) is True

    def test_empty_array_falsy(self) -> None:
        doc = Document.parse("arr = []\n")
        assert bool(doc["arr"]) is False

    def test_nonempty_table_truthy(self) -> None:
        doc = make_doc()
        assert bool(doc["owner"]) is True

    def test_empty_table_falsy(self) -> None:
        doc = Document.parse("[empty]\n")
        assert bool(doc["empty"]) is False

    def test_scalar_truthy(self) -> None:
        doc = make_doc()
        assert bool(doc["title"]) is True

    def test_inline_table_falsy_when_empty(self) -> None:
        doc = Document.parse("meta = {}\n")
        assert bool(doc["meta"]) is False

    def test_zero_int_falsy(self) -> None:
        doc = Document.parse("x = 0\n")
        assert bool(doc["x"]) is False

    def test_nonzero_int_truthy(self) -> None:
        doc = Document.parse("x = 42\n")
        assert bool(doc["x"]) is True

    def test_zero_float_falsy(self) -> None:
        doc = Document.parse("x = 0.0\n")
        assert bool(doc["x"]) is False

    def test_nonzero_float_truthy(self) -> None:
        doc = Document.parse("x = 3.14\n")
        assert bool(doc["x"]) is True

    def test_false_bool_falsy(self) -> None:
        doc = Document.parse("x = false\n")
        assert bool(doc["x"]) is False

    def test_true_bool_truthy(self) -> None:
        doc = Document.parse("x = true\n")
        assert bool(doc["x"]) is True

    def test_empty_string_falsy(self) -> None:
        doc = Document.parse('x = ""\n')
        assert bool(doc["x"]) is False

    def test_nonempty_string_truthy(self) -> None:
        doc = Document.parse('x = "hello"\n')
        assert bool(doc["x"]) is True


# ---------------------------------------------------------------------------
# Item: __delitem__
# ---------------------------------------------------------------------------


class TestProxyDelitem:
    def test_del_table_key(self) -> None:
        doc = make_doc()
        del doc["owner"]["active"]
        assert "active" not in doc["owner"]
        assert len(doc["owner"]) == 2

    def test_del_array_element(self) -> None:
        doc = make_doc()
        del doc["database"]["ports"][0]
        assert len(doc["database"]["ports"]) == 2

    def test_del_inline_table_key(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        del doc["meta"]["x"]
        assert "x" not in doc["meta"]

    def test_del_missing_table_key_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(KeyError):
            del doc["owner"]["nonexistent"]

    def test_del_array_out_of_bounds_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(IndexError):
            del doc["database"]["ports"][99]


# ---------------------------------------------------------------------------
# Item: __repr__
# ---------------------------------------------------------------------------


class TestProxyRepr:
    def test_repr_includes_type(self) -> None:
        doc = make_doc()
        r = repr(doc["owner"])
        assert "Item" in r

    def test_repr_includes_content(self) -> None:
        doc = make_doc()
        r = repr(doc["title"])
        assert "Example" in r

    def test_repr_scalar(self) -> None:
        doc = make_doc()
        r = repr(doc["owner"]["age"])
        assert "30" in r


# ---------------------------------------------------------------------------
# Item: dict-like methods (keys, values, items, get, pop, update, etc.)
# ---------------------------------------------------------------------------


class TestProxyDictMethods:
    def test_keys(self) -> None:
        doc = make_doc()
        assert set(doc["owner"].keys()) == {"name", "age", "active"}

    def test_values(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        vals = doc["t"].values()
        assert len(vals) == 2

    def test_values_are_live_proxies(self) -> None:
        doc = Document.parse("[t]\n[t.inner]\nval = 10\n")
        vals = doc["t"].values()
        # vals[0] is a proxy to doc["t"]["inner"], a sub-table
        vals[0]["val"] = 99
        assert doc["t"]["inner"]["val"] == 99

    def test_items(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        pairs = doc["t"].items()
        keys = [k for k, v in pairs]
        assert set(keys) == {"a", "b"}

    def test_items_returns_live_proxies(self) -> None:
        doc = Document.parse("[t]\n[t.inner]\nval = 10\n")
        for key, proxy in doc["t"].items():
            if key == "inner":
                proxy["val"] = 99
        assert doc["t"]["inner"]["val"] == 99

    def test_get_existing(self) -> None:
        doc = make_doc()
        assert doc["owner"].get("name") == "Alice"

    def test_get_missing(self) -> None:
        doc = make_doc()
        assert doc["owner"].get("email") is None

    def test_get_returns_live_proxy(self) -> None:
        doc = Document.parse("[t]\n[t.inner]\nval = 10\n")
        inner = doc["t"].get("inner")
        assert inner is not None
        inner["val"] = 42
        assert doc["t"]["inner"]["val"] == 42

    def test_pop_table_key(self) -> None:
        doc = make_doc()
        doc["owner"].pop("active")
        assert "active" not in doc["owner"]

    def test_pop_missing_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(KeyError):
            doc["owner"].pop("nonexistent")

    def test_pop_missing_with_default(self) -> None:
        doc = make_doc()
        assert doc["owner"].pop("nonexistent", 42) == 42

    def test_pop_existing_ignores_default(self) -> None:
        doc = make_doc()
        val = doc["owner"].pop("age", 99)
        assert val == 30
        assert "age" not in doc["owner"]

    def test_update(self) -> None:
        doc = make_doc()
        doc["owner"].update({"name": "Bob", "email": "bob@example.com"})
        assert doc["owner"]["name"] == "Bob"
        assert doc["owner"]["email"] == "bob@example.com"

    def test_setdefault_missing(self) -> None:
        doc = make_doc()
        result = doc["owner"].setdefault("email", "default@example.com")
        assert result == "default@example.com"
        assert doc["owner"]["email"] == "default@example.com"

    def test_setdefault_existing(self) -> None:
        doc = make_doc()
        result = doc["owner"].setdefault("name", "fallback")
        assert result == "Alice"  # not overwritten

    def test_clear_table(self) -> None:
        doc = make_doc()
        doc["owner"].clear()
        assert len(doc["owner"]) == 0

    def test_inline_table_keys(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert set(doc["meta"].keys()) == {"x", "y"}

    def test_inline_table_get(self) -> None:
        doc = Document.parse("meta = {x = 1}\n")
        assert doc["meta"].get("x") == 1
        assert doc["meta"].get("z") is None

    def test_inline_table_pop(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        doc["meta"].pop("x")
        assert "x" not in doc["meta"]

    def test_inline_table_update(self) -> None:
        doc = Document.parse("meta = {x = 1}\n")
        doc["meta"].update({"y": 2})
        assert doc["meta"]["y"] == 2

    def test_keys_on_scalar_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(TypeError):
            doc["title"].keys()


# ---------------------------------------------------------------------------
# Item: list-like methods (append, insert, pop, remove, extend, clear)
# ---------------------------------------------------------------------------


class TestProxyListMethods:
    def test_append(self) -> None:
        doc = make_doc()
        doc["database"]["ports"].append(9999)
        assert len(doc["database"]["ports"]) == 4
        assert doc["database"]["ports"][3] == 9999

    def test_insert_at_beginning(self) -> None:
        doc = Document.parse("arr = [2, 3]\n")
        doc["arr"].insert(0, 1)
        assert doc["arr"][0] == 1
        assert len(doc["arr"]) == 3

    def test_insert_at_end(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        doc["arr"].insert(2, 3)
        assert doc["arr"][2] == 3

    def test_insert_out_of_range_clamps(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].insert(100, 4)
        assert doc["arr"] == [1, 2, 3, 4]

    def test_insert_negative_index(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].insert(-1, 0)
        assert doc["arr"] == [1, 2, 0, 3]

    def test_insert_very_negative_clamps_to_zero(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].insert(-100, 0)
        assert doc["arr"] == [0, 1, 2, 3]

    def test_pop_last(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].pop()
        assert len(doc["arr"]) == 2

    def test_pop_by_index(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].pop(0)
        assert doc["arr"][0] == 2

    def test_pop_empty_raises(self) -> None:
        doc = Document.parse("arr = []\n")
        with pytest.raises(IndexError):
            doc["arr"].pop()

    def test_remove_value(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].remove(2)
        assert len(doc["arr"]) == 2
        assert doc["arr"][0] == 1
        assert doc["arr"][1] == 3

    def test_remove_missing_raises(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].remove(99)

    def test_extend(self) -> None:
        doc = Document.parse("arr = [1]\n")
        doc["arr"].extend([2, 3, 4])
        assert len(doc["arr"]) == 4
        assert doc["arr"][3] == 4

    def test_clear_array(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].clear()
        assert len(doc["arr"]) == 0

    def test_append_on_table_raises(self) -> None:
        doc = make_doc()
        with pytest.raises(TypeError):
            doc["owner"].append(1)

    def test_remove_string(self) -> None:
        doc = Document.parse('arr = ["a", "b", "c"]\n')
        doc["arr"].remove("b")
        assert len(doc["arr"]) == 2
        assert doc["arr"][0] == "a"
        assert doc["arr"][1] == "c"


# ---------------------------------------------------------------------------
# pop() returns native Python values
# ---------------------------------------------------------------------------


class TestPopReturnsNative:
    """pop() should return native Python objects, not internal Item wrappers."""

    def test_pop_table_returns_dict(self) -> None:
        doc = make_doc()
        owner = doc.pop("owner")
        assert isinstance(owner, dict)
        assert owner == {"name": "Alice", "age": 30, "active": True}

    def test_pop_array_returns_list(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        arr = doc.pop("arr")
        assert isinstance(arr, list)
        assert arr == [1, 2, 3]

    def test_pop_string_returns_str(self) -> None:
        doc = Document.parse('x = "hello"\n')
        assert doc.pop("x") == "hello"

    def test_pop_integer_returns_int(self) -> None:
        doc = Document.parse("x = 42\n")
        assert doc.pop("x") == 42

    def test_pop_float_returns_float(self) -> None:
        doc = Document.parse("x = 9.81\n")
        v = doc.pop("x")
        assert v == 9.81
        assert type(v) is float

    def test_pop_bool_returns_bool(self) -> None:
        doc = Document.parse("x = true\n")
        assert doc.pop("x") is True

    def test_pop_datetime_returns_datetime(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        v = doc.pop("dt")
        assert type(v) is datetime

    def test_pop_nested_table(self) -> None:
        doc = Document.parse("[a]\n[a.b]\nx = 1\n")
        assert doc.pop("a") == {"b": {"x": 1}}

    def test_pop_missing_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(KeyError):
            doc.pop("nope")

    def test_pop_missing_with_default(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.pop("nope", 42) == 42

    def test_pop_removes_key(self) -> None:
        doc = make_doc()
        doc.pop("owner")
        assert "owner" not in doc

    def test_proxy_pop_array_element(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        v = doc["arr"].pop()
        assert v == 3
        assert doc["arr"] == [1, 2]

    def test_proxy_pop_by_key(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        v = doc["t"].pop("a")
        assert v == 1
        assert "a" not in doc["t"]


# ---------------------------------------------------------------------------
# Error handling: no panics, only proper Python exceptions
# ---------------------------------------------------------------------------


class TestErrorHandling:
    def test_setitem_on_string_raises_type_error(self) -> None:
        doc = Document.parse('version = "0.0.1"\n')
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["version"][3] = 12

    def test_setitem_on_int_raises_type_error(self) -> None:
        doc = Document.parse("count = 42\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["count"][0] = 1

    def test_setitem_on_bool_raises_type_error(self) -> None:
        doc = Document.parse("flag = true\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["flag"][0] = False

    def test_setitem_on_float_raises_type_error(self) -> None:
        doc = Document.parse("val = 1.5\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["val"][0] = 2

    def test_getitem_on_scalar_raises_key_error(self) -> None:
        doc = Document.parse('name = "hello"\n')
        with pytest.raises(KeyError):
            doc["name"]["x"]

    def test_delitem_on_scalar_raises_type_error(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            del doc["val"][0]

    def test_len_on_scalar_raises_type_error(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            len(doc["val"])

    def test_iter_on_scalar_raises_type_error(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            iter(doc["val"])

    def test_contains_on_scalar_raises_type_error(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            assert 1 in doc["val"]

    def test_append_on_table_raises_type_error(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="append"):
            doc["t"].append(1)

    def test_keys_on_array_raises_type_error(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="keys"):
            doc["arr"].keys()

    def test_bad_key_type_raises_type_error(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            doc["tbl"][1.5]  # type: ignore[call-overload]

    def test_assign_none_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(Exception, match=r"None.*not a valid TOML"):
            doc["x"] = None


class TestDocumentCompleteness:
    """Document should have a complete dict-like API."""

    def test_repr(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert repr(doc) == "Document(2 keys)"

    def test_repr_empty(self) -> None:
        doc = Document.parse("")
        assert repr(doc) == "Document(0 keys)"

    def test_bool_nonempty(self) -> None:
        doc = Document.parse("x = 1\n")
        assert bool(doc) is True

    def test_bool_empty(self) -> None:
        doc = Document.parse("")
        assert bool(doc) is False

    def test_eq_same_content(self) -> None:
        a = Document.parse("x = 1\ny = 2\n")
        b = Document.parse("x = 1\ny = 2\n")
        assert a == b

    def test_eq_different_content(self) -> None:
        a = Document.parse("x = 1\n")
        b = Document.parse("x = 2\n")
        assert a != b

    def test_eq_dict(self) -> None:
        doc = Document.parse("x = 1\ny = 2\n")
        assert doc == {"x": 1, "y": 2}
        assert doc != {"x": 1}

    def test_eq_unrelated_type(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc != 42
        assert doc != "x = 1"

    def test_eq_doc_vs_doc_type_strict(self) -> None:
        """Document-to-Document equality should respect TOML types."""
        a = Document.parse("x = true\n")
        b = Document.parse("x = 1\n")
        assert a != b

    def test_delitem_raises_key_error(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(KeyError):
            del doc["nonexistent"]

    def test_delitem_existing_key(self) -> None:
        doc = Document.parse("x = 1\ny = 2\n")
        del doc["x"]
        assert "x" not in doc
        assert "y" in doc

    def test_update(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update({"x": 10, "y": 20})
        assert doc["x"] == 10
        assert doc["y"] == 20

    def test_setdefault_missing(self) -> None:
        doc = Document.parse("x = 1\n")
        result = doc.setdefault("y", 42)
        assert result == 42
        assert doc["y"] == 42

    def test_setdefault_existing(self) -> None:
        doc = Document.parse("x = 1\n")
        result = doc.setdefault("x", 99)
        assert result == 1


class TestNegativeIndexing:
    """Negative indices should work like Python lists."""

    def test_proxy_getitem_minus_one(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        assert doc["arr"][-1] == 30

    def test_proxy_getitem_minus_two(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        assert doc["arr"][-2] == 20

    def test_proxy_setitem_negative(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        doc["arr"][-1] = 99
        assert doc["arr"][-1] == 99
        assert doc["arr"][2] == 99

    def test_proxy_delitem_negative(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        del doc["arr"][-1]
        assert doc["arr"] == [10, 20]

    def test_proxy_pop_negative(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        val = doc["arr"].pop(-2)
        assert val == 20
        assert doc["arr"] == [10, 30]

    def test_out_of_range_negative(self) -> None:
        doc = Document.parse("arr = [10, 20]\n")
        with pytest.raises(IndexError):
            doc["arr"][-3]


class TestGetWithDefault:
    """get() should accept an optional default value."""

    def test_doc_get_existing(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.get("x") == 1

    def test_doc_get_missing_no_default(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.get("y") is None

    def test_doc_get_missing_with_default(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.get("y", 42) == 42

    def test_proxy_get_existing(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        assert doc["tbl"].get("x") == 1

    def test_proxy_get_missing_no_default(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        assert doc["tbl"].get("y") is None

    def test_proxy_get_missing_with_default(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        assert doc["tbl"].get("y", "fallback") == "fallback"


class TestPopWithDefault:
    """Document.pop() should accept an optional default."""

    def test_pop_existing(self) -> None:
        doc = Document.parse("x = 1\ny = 2\n")
        val = doc.pop("x")
        assert val == 1
        assert "x" not in doc

    def test_pop_missing_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(KeyError):
            doc.pop("y")

    def test_pop_missing_with_default(self) -> None:
        doc = Document.parse("x = 1\n")
        val = doc.pop("y", 42)
        assert val == 42
        assert "y" not in doc


class TestSliceIndexing:
    """Slice support on arrays via __getitem__, __setitem__, __delitem__."""

    TOML = "arr = [1, 2, 3, 4, 5]\n"

    # ---- __getitem__ slices ----

    def test_basic_slice(self) -> None:
        doc = Document.parse(self.TOML)
        result = doc["arr"][1:3]
        assert isinstance(result, list)
        assert len(result) == 2
        assert result[0] == 2
        assert result[1] == 3

    def test_slice_from_start(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][:3]] == [1, 2, 3]

    def test_slice_to_end(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][3:]] == [4, 5]

    def test_full_slice(self) -> None:
        doc = Document.parse(self.TOML)
        result = doc["arr"][:]
        assert len(result) == 5
        assert result[0] == 1
        assert result[4] == 5

    def test_negative_start(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][-2:]] == [4, 5]

    def test_negative_stop(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][:-2]] == [1, 2, 3]

    def test_step(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][::2]] == [1, 3, 5]

    def test_negative_step_reverse(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][::-1]] == [5, 4, 3, 2, 1]

    def test_empty_slice(self) -> None:
        doc = Document.parse(self.TOML)
        assert doc["arr"][2:2] == []

    def test_out_of_range_slice_clamps(self) -> None:
        doc = Document.parse(self.TOML)
        result = doc["arr"][3:100]
        assert [int(str(x)) for x in result] == [4, 5]

    def test_slice_returns_proxies(self) -> None:
        """Each element of the returned list is still a live proxy."""
        doc = Document.parse(self.TOML)
        doc["arr"][1:3]
        # They're proxies, so mutating the doc should be reflected
        doc["arr"][1] = 20
        # Re-read via fresh proxy
        assert doc["arr"][1] == 20

    # ---- __setitem__ slices ----

    def test_setitem_same_length(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][1:3] = [20, 30]
        assert doc["arr"] == [1, 20, 30, 4, 5]

    def test_setitem_grow(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][1:3] = [20, 30, 40]
        assert doc["arr"] == [1, 20, 30, 40, 4, 5]

    def test_setitem_shrink(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][1:4] = [99]
        assert doc["arr"] == [1, 99, 5]

    def test_setitem_empty_replacement(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][1:3] = []
        assert doc["arr"] == [1, 4, 5]

    def test_setitem_insert_at_position(self) -> None:
        """Setting an empty slice inserts without removing."""
        doc = Document.parse(self.TOML)
        doc["arr"][2:2] = [10, 11]
        assert doc["arr"] == [1, 2, 10, 11, 3, 4, 5]

    def test_setitem_extended_slice(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][0:5:2] = [10, 30, 50]
        assert doc["arr"] == [10, 2, 30, 4, 50]

    def test_setitem_extended_slice_mismatch_raises(self) -> None:
        doc = Document.parse(self.TOML)
        with pytest.raises(ValueError, match="extended slice"):
            doc["arr"][0:5:2] = [10, 30]

    # ---- __delitem__ slices ----

    def test_delitem_basic(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][1:3]
        assert doc["arr"] == [1, 4, 5]

    def test_delitem_from_start(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][:2]
        assert doc["arr"] == [3, 4, 5]

    def test_delitem_to_end(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][3:]
        assert doc["arr"] == [1, 2, 3]

    def test_delitem_step(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][::2]
        assert doc["arr"] == [2, 4]

    def test_delitem_empty_slice_noop(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][2:2]
        assert doc["arr"] == [1, 2, 3, 4, 5]

    def test_delitem_all(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][:]
        assert len(doc["arr"]) == 0

    # ---- errors ----

    def test_slice_on_table_raises(self) -> None:
        doc = Document.parse("[t]\nx = 1\n")
        with pytest.raises(TypeError, match="does not support slicing"):
            doc["t"][1:3]

    def test_slice_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 'hello'\n")
        with pytest.raises(TypeError, match="does not support slicing"):
            doc["x"][1:3]

    # ---- mutation visible in document ----

    def test_setitem_slice_visible_in_output(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][1:4] = [20, 30, 40]
        assert "20" in str(doc)
        assert "30" in str(doc)
        assert "40" in str(doc)

    def test_delitem_slice_visible_in_output(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][0:2]
        output = str(doc)
        assert "1" not in output.split("=")[1]  # 1 removed


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

    def test_table(self) -> None:
        doc = Document.parse(self.TOML)
        v = doc["tbl"].value
        assert v == {"x": 1, "y": 2}
        assert type(v) is dict

    def test_nested_array(self) -> None:
        doc = Document.parse("arr = [[1, 2], [3, 4]]\n")
        v = doc["arr"].value
        assert v == [[1, 2], [3, 4]]

    def test_nested_table(self) -> None:
        doc = Document.parse("[a]\n[a.b]\nx = 1\n")
        v = doc["a"].value
        assert v == {"b": {"x": 1}}

    def test_augmented_assignment(self) -> None:
        """The motivating use case: doc['count'] += 4."""
        doc = Document.parse("count = 2\n")
        doc["count"] = doc["count"].value + 4
        assert doc["count"] == 6

    def test_pop_returns_native(self) -> None:
        """pop() returns a native value, not an Item wrapper."""
        doc = Document.parse("x = 99\n")
        v = doc.pop("x")
        assert v == 99
        assert type(v) is int

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


class TestDocumentConstructor:
    """Document() can create empty or from a table."""

    def test_empty(self) -> None:
        doc = Document()
        assert len(doc) == 0
        assert not str(doc)

    def test_empty_then_populate(self) -> None:
        doc = Document()
        doc["name"] = "test"
        doc["version"] = 1
        assert doc["name"] == "test"
        assert doc["version"] == 1

    def test_from_dict(self) -> None:
        doc = Document({"a": 1, "b": "two"})
        assert doc["a"] == 1
        assert doc["b"] == "two"

    def test_from_popped_table(self) -> None:
        src = Document.parse("[project]\nname = 'hello'\nversion = '1.0'\n")
        data = src.pop("project")
        doc = Document(data)
        assert doc["name"] == "hello"
        assert doc["version"] == "1.0"

    def test_non_dict_raises(self) -> None:
        with pytest.raises(TypeError, match="must be a dict"):
            Document(42)  # type: ignore[arg-type]

    def test_none_is_empty(self) -> None:
        doc = Document(None)
        assert len(doc) == 0


class TestDocumentValue:
    """Document.value returns the entire document as a native Python dict."""

    def test_simple(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        v = doc.value
        assert v == {"a": 1, "b": 2}
        assert type(v) is dict

    def test_nested(self) -> None:
        doc = Document.parse("[section]\nx = 1\ny = 2\n")
        assert doc.value == {"section": {"x": 1, "y": 2}}

    def test_empty(self) -> None:
        doc = Document()
        assert doc.value == {}

    def test_complex(self) -> None:
        doc = make_doc()
        v = doc.value
        assert v["title"] == "Example"
        assert v["owner"] == {"name": "Alice", "age": 30, "active": True}
        assert v["database"]["ports"] == [8001, 8001, 8002]

    def test_value_is_a_copy(self) -> None:
        doc = Document.parse("x = 1\n")
        v = doc.value
        v["x"] = 999
        assert doc["x"] == 1


class TestComment:
    """Tests for the .comment (block) and .inline_comment (inline) properties."""

    # ---- reading inline comment ----

    def test_read_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # inline note\n")
        assert doc["key"].inline_comment == "# inline note"

    def test_read_no_inline_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        assert doc["key"].inline_comment is None

    def test_read_inline_on_string(self) -> None:
        doc = Document.parse('name = "Alice" # person\n')
        assert doc["name"].inline_comment == "# person"

    def test_read_inline_on_table_value(self) -> None:
        doc = Document.parse("[section]\nx = 1 # inside\n")
        assert doc["section"]["x"].inline_comment == "# inside"

    # ---- setting inline comment ----

    def test_set_inline_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "# added"
        assert "# added" in str(doc)
        assert doc["key"].inline_comment == "# added"

    def test_replace_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # old\n")
        doc["key"].inline_comment = "# new"
        assert doc["key"].inline_comment == "# new"
        assert "# old" not in str(doc)

    def test_clear_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # remove me\n")
        doc["key"].inline_comment = None
        assert doc["key"].inline_comment is None
        assert "#" not in str(doc)

    def test_inline_comment_preserves_value(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "# note"
        assert doc["key"] == 42

    # ---- reading comment ----

    def test_read_comment(self) -> None:
        doc = Document.parse("# before\nkey = 42\n")
        assert doc["key"].comment == "# before"

    def test_read_no_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        assert doc["key"].comment is None

    def test_read_multiline_comment(self) -> None:
        doc = Document.parse("# line 1\n# line 2\nkey = 42\n")
        assert doc["key"].comment == "# line 1\n# line 2"

    def test_read_comment_nested(self) -> None:
        doc = Document.parse("[section]\n# before x\nx = 1\n")
        assert doc["section"]["x"].comment == "# before x"

    # ---- setting comment ----

    def test_set_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# added"
        assert "# added" in str(doc)
        assert doc["key"].comment == "# added"

    def test_replace_comment(self) -> None:
        doc = Document.parse("# old\nkey = 42\n")
        doc["key"].comment = "# new"
        assert doc["key"].comment == "# new"
        assert "# old" not in str(doc)

    def test_clear_comment(self) -> None:
        doc = Document.parse("# remove\nkey = 42\n")
        doc["key"].comment = None
        assert doc["key"].comment is None

    def test_multiline_comment_set(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# line A\n# line B"
        s = str(doc)
        assert "# line A\n# line B\n" in s
        assert doc["key"].comment == "# line A\n# line B"

    # ---- both together ----

    def test_both_comments(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# above"
        doc["key"].inline_comment = "# inline"
        s = str(doc)
        assert "# above" in s
        assert "# inline" in s
        assert doc["key"].comment == "# above"
        assert doc["key"].inline_comment == "# inline"

    # ---- round-trip ----

    def test_comment_roundtrip(self) -> None:
        toml = "# important\nkey = 42 # magic number\n"
        doc = Document.parse(toml)
        assert str(doc) == toml

    # ---- error cases ----

    def test_comment_on_array_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        assert doc["arr"][0].comment is None
        doc["arr"][1].comment = "# about two"
        s = str(doc)
        assert "# about two" in s
        assert "  # about two\n  2" in s

    def test_inline_comment_rejects_newlines(self) -> None:
        doc = Document.parse("key = 42\n")
        with pytest.raises(ValueError, match="newlines"):
            doc["key"].inline_comment = "# line1\nline2"

    def test_set_inline_comment_on_array_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][2].inline_comment = "# last"
        s = str(doc)
        assert "1, # first" in s
        assert "3, # last" in s
        doc2 = Document.parse(s)
        assert doc2["arr"][0].inline_comment == "# first"
        assert doc2["arr"][1].inline_comment is None
        assert doc2["arr"][2].inline_comment == "# last"

    def test_array_inline_comment_roundtrip(self) -> None:
        toml = 'arr = [\n  "a", # one\n  "b", # two\n  "c", # three\n]\n'
        doc = Document.parse(toml)
        assert str(doc) == toml
        assert doc["arr"][0].inline_comment == "# one"
        assert doc["arr"][1].inline_comment == "# two"
        assert doc["arr"][2].inline_comment == "# three"

    def test_array_inline_comment_clear(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][1].inline_comment = "# temp"
        assert doc["arr"][1].inline_comment == "# temp"
        doc["arr"][1].inline_comment = None
        assert doc["arr"][1].inline_comment is None
        assert "# temp" not in str(doc)

    def test_array_inline_comment_replace(self) -> None:
        toml = "arr = [\n  1, # old\n  2,\n]\n"
        doc = Document.parse(toml)
        assert doc["arr"][0].inline_comment == "# old"
        doc["arr"][0].inline_comment = "# new"
        assert doc["arr"][0].inline_comment == "# new"
        assert "# old" not in str(doc)
        assert "# new" in str(doc)

    def test_array_inline_comment_preserves_values(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        doc["arr"][0].inline_comment = "# note"
        assert doc["arr"][0] == 1
        assert doc["arr"][1] == 2

    def test_array_inline_comment_rejects_newlines(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        with pytest.raises(ValueError, match="newlines"):
            doc["arr"][0].inline_comment = "# a\nb"

    def test_array_inline_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][0].inline_comment = "# wide indent"
        s = str(doc)
        assert "\n    2," in s  # 4-space indent preserved

    def test_array_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][1].comment = "# about two"
        s = str(doc)
        assert "    # about two\n    2" in s

    def test_array_comment_clear(self) -> None:
        toml = "arr = [\n  1,\n  # note\n  2,\n]\n"
        doc = Document.parse(toml)
        doc["arr"][1].comment = None
        assert "# note" not in str(doc)

    def test_array_inline_comment_on_single_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n]\n")
        doc["arr"][0].inline_comment = "# only"
        s = str(doc)
        assert "1, # only" in s
        doc2 = Document.parse(s)
        assert doc2["arr"][0].inline_comment == "# only"

    def test_array_comment_valid_toml(self) -> None:
        """Every array comment operation should produce re-parseable TOML."""
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][1].comment = "# block"
        doc["arr"][2].inline_comment = "# last"
        Document.parse(str(doc))  # should not raise

    def test_array_inline_and_block_coexist(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# inline on first"
        doc["arr"][1].comment = "# block before second"
        s = str(doc)
        assert "1, # inline on first" in s
        assert "  # block before second\n  2" in s
        assert doc["arr"][0].inline_comment == "# inline on first"
        assert doc["arr"][1].comment == "# block before second"
        Document.parse(s)  # valid TOML

    def test_array_inline_and_block_native_roundtrip(self) -> None:
        toml = "arr = [\n  1, # inline\n  # block\n  2,\n]\n"
        doc = Document.parse(toml)
        assert str(doc) == toml
        assert doc["arr"][0].inline_comment == "# inline"
        assert doc["arr"][1].comment == "# block"

    # ---- blank lines ----

    def test_blank_line_before_comment_roundtrip(self) -> None:
        doc = Document.parse("a = 1\n\n# note\nb = 2\n")
        assert doc["b"].comment == "\n# note"
        assert str(doc) == "a = 1\n\n# note\nb = 2\n"

    def test_set_blank_line_before_comment(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        doc["b"].comment = "\n# note"
        assert str(doc) == "a = 1\n\n# note\nb = 2\n"
        assert doc["b"].comment == "\n# note"

    def test_multiple_blank_lines_before_comment(self) -> None:
        doc = Document.parse("a = 1\n\n\n# note\nb = 2\n")
        assert doc["b"].comment == "\n\n# note"

    def test_blank_line_between_comment_lines(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        doc["b"].comment = "# first\n\n# second"
        s = str(doc)
        assert "# first\n\n# second\n" in s
        assert doc["b"].comment == "# first\n\n# second"

    # ---- validation ----

    def test_inline_comment_requires_hash(self) -> None:
        doc = Document.parse("key = 42\n")
        with pytest.raises(ValueError, match="#"):
            doc["key"].inline_comment = "no hash"

    def test_comment_requires_hash(self) -> None:
        doc = Document.parse("key = 42\n")
        with pytest.raises(ValueError, match="#"):
            doc["key"].comment = "no hash"

    def test_lone_hash_inline(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "#"
        assert "42 #" in str(doc)
        assert doc["key"].inline_comment == "#"

    def test_lone_hash_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "#"
        s = str(doc)
        assert "#\nkey" in s
        assert doc["key"].comment == "#"

    def test_array_inline_comment_requires_hash(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        with pytest.raises(ValueError, match="#"):
            doc["arr"][0].inline_comment = "no hash"

    def test_array_comment_requires_hash(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        with pytest.raises(ValueError, match="#"):
            doc["arr"][1].comment = "no hash"

    def test_inline_comment_on_aot_rejected(self) -> None:
        doc = Document.parse("[[s]]\nname = 'a'\n[[s]]\nname = 'b'\n")
        with pytest.raises(TypeError, match="does not support"):
            doc["s"].inline_comment = "# nope"

    def test_comment_on_aot_element_rejected(self) -> None:
        doc = Document.parse("[[s]]\nname = 'a'\n[[s]]\nname = 'b'\n")
        with pytest.raises(TypeError, match="does not support"):
            doc["s"][0].comment = "# nope"
