"""Tests for Item proxy: dict-like methods."""

from __future__ import annotations

from collections.abc import ItemsView, KeysView, MutableMapping, ValuesView
from datetime import datetime
from types import MappingProxyType

import pytest

from tomledit import Document

# ---------------------------------------------------------------------------
# Proxy dict-like methods (keys, values, items, get, pop, update, etc.)
# ---------------------------------------------------------------------------


class TestProxyDictMethods:
    # -- keys / values / items --

    def test_keys(self, doc: Document) -> None:
        assert set(doc["owner"].keys()) == {"name", "age", "active"}

    def test_values(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        vals = doc["t"].values()
        assert len(vals) == 2

    def test_values_are_live_proxies(self) -> None:
        doc = Document.parse("[t]\n[t.inner]\nval = 10\n")
        vals = list(doc["t"].values())
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

    # -- get --

    def test_get_returns_live_proxy(self) -> None:
        doc = Document.parse("[t]\n[t.inner]\nval = 10\n")
        inner = doc["t"].get("inner")
        assert inner is not None
        inner["val"] = 42
        assert doc["t"]["inner"]["val"] == 42

    def test_get_existing(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        assert doc["tbl"].get("x") == 1

    def test_get_missing_no_default(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        assert doc["tbl"].get("y") is None

    def test_get_missing_with_default(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        assert doc["tbl"].get("y", "fallback") == "fallback"

    # -- pop --

    def test_pop_table_key(self, doc: Document) -> None:
        doc["owner"].pop("active")
        assert "active" not in doc["owner"]

    def test_pop_missing_raises(self, doc: Document) -> None:
        with pytest.raises(KeyError):
            doc["owner"].pop("nonexistent")

    def test_pop_missing_with_default(self, doc: Document) -> None:
        assert doc["owner"].pop("nonexistent", 42) == 42

    def test_pop_missing_with_none_default(self, doc: Document) -> None:
        assert doc["owner"].pop("nonexistent", None) is None

    def test_pop_existing_ignores_default(self, doc: Document) -> None:
        val = doc["owner"].pop("age", 99)
        assert val == 30
        assert "age" not in doc["owner"]

    def test_pop_too_many_args(self) -> None:
        doc = Document.parse("[owner]\nname = 'Tom'\n")
        with pytest.raises(TypeError, match="at most 2 arguments"):
            doc["owner"].pop("name", 1, 2)

    def test_pop_by_key_returns_native(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        v = doc["t"].pop("a")
        assert v == 1
        assert "a" not in doc["t"]

    def test_pop_array_element_returns_native(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        v = doc["arr"].pop()
        assert v == 3
        assert doc["arr"] == [1, 2]

    # -- update --

    def test_update(self, doc: Document) -> None:
        doc["owner"].update({"name": "Bob", "email": "bob@example.com"})
        assert doc["owner"]["name"] == "Bob"
        assert doc["owner"]["email"] == "bob@example.com"

    def test_update_kwargs(self, doc: Document) -> None:
        doc["owner"].update(name="Bob", email="bob@example.com")
        assert doc["owner"]["name"] == "Bob"
        assert doc["owner"]["email"] == "bob@example.com"

    def test_update_iterable_of_pairs(self, doc: Document) -> None:
        doc["owner"].update([("name", "Bob"), ("email", "bob@example.com")])
        assert doc["owner"]["name"] == "Bob"
        assert doc["owner"]["email"] == "bob@example.com"

    # -- setdefault --

    def test_setdefault_missing(self, doc: Document) -> None:
        result = doc["owner"].setdefault("email", "default@example.com")
        assert result == "default@example.com"
        assert doc["owner"]["email"] == "default@example.com"

    def test_setdefault_existing(self, doc: Document) -> None:
        result = doc["owner"].setdefault("name", "fallback")
        assert result == "Alice"  # not overwritten

    def test_setdefault_no_default_existing(self, doc: Document) -> None:
        result = doc["owner"].setdefault("name")
        assert result == "Alice"

    def test_setdefault_no_default_missing(self, doc: Document) -> None:
        with pytest.raises(TypeError, match="TOML has no null type"):
            doc["owner"].setdefault("email")

    # -- clear --

    def test_clear_table(self, doc: Document) -> None:
        doc["owner"].clear()
        assert len(doc["owner"]) == 0

    def test_clear_inline_table(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        doc["t"].clear()
        assert len(doc["t"]) == 0
        assert doc["t"] == {}

    # -- inline table specifics --

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

    # -- errors --

    def test_keys_on_scalar_raises(self, doc: Document) -> None:
        with pytest.raises(AttributeError):
            doc["title"].keys()


# ---------------------------------------------------------------------------
# Document-level dict methods (get, pop with defaults, return types)
# ---------------------------------------------------------------------------


class TestDocumentDictMethods:
    # -- contains / len / iter / keys --

    def test_contains(self, doc: Document) -> None:
        assert "title" in doc
        assert "nonexistent" not in doc

    def test_len(self, doc: Document) -> None:
        assert len(doc) == 4  # title, owner, database, servers

    def test_iter(self, doc: Document) -> None:
        keys = list(doc)
        assert "title" in keys
        assert "owner" in keys

    def test_keys(self, doc: Document) -> None:
        assert set(doc.keys()) == {"title", "owner", "database", "servers"}

    # -- getitem --

    def test_getitem_missing_raises(self, doc: Document) -> None:
        with pytest.raises(KeyError):
            doc["nope"]

    # -- get --

    def test_get_existing(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.get("x") == 1

    def test_get_missing_no_default(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.get("y") is None

    def test_get_missing_with_default(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.get("y", 42) == 42

    def test_get_returns_live_proxy(self, doc: Document) -> None:
        owner = doc.get("owner")
        assert owner is not None
        owner["name"] = "Bob"
        assert doc["owner"]["name"] == "Bob"

    # -- delitem --

    def test_delitem_existing_key(self) -> None:
        doc = Document.parse("x = 1\ny = 2\n")
        del doc["x"]
        assert "x" not in doc
        assert "y" in doc

    def test_delitem_raises_key_error(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(KeyError):
            del doc["nonexistent"]

    # -- pop argument handling --

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

    def test_pop_missing_with_none_default(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.pop("y", None) is None

    def test_pop_too_many_args(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="at most 2 arguments"):
            doc.pop("x", 1, 2)  # type: ignore[call-overload]

    # -- pop return types --

    def test_pop_table_returns_dict(self, doc: Document) -> None:
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

    # -- items / values return live proxies --

    def test_items_returns_live_proxies(self, doc: Document) -> None:
        for key, proxy in doc.items():
            if key == "owner":
                proxy["name"] = "Charlie"
                break
        assert doc["owner"]["name"] == "Charlie"

    def test_values_returns_live_proxies(self) -> None:
        doc = Document.parse("[section]\nval = 10\n")
        vals = list(doc.values())
        assert len(vals) == 1
        vals[0]["val"] = 99
        assert doc["section"]["val"] == 99

    # -- update --

    def test_update(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update({"x": 10, "y": 20})
        assert doc["x"] == 10
        assert doc["y"] == 20

    def test_update_kwargs(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update(x=10, y=20)
        assert doc["x"] == 10
        assert doc["y"] == 20

    def test_update_dict_and_kwargs(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update({"x": 10}, y=20)
        assert doc["x"] == 10
        assert doc["y"] == 20

    def test_update_iterable_of_pairs(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update([("x", 10), ("y", 20)])
        assert doc["x"] == 10
        assert doc["y"] == 20

    def test_update_no_args(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update()
        assert doc["x"] == 1

    def test_update_mapping_with_keys(self) -> None:
        doc = Document.parse("x = 1\n")
        doc.update(MappingProxyType({"x": 10, "y": 20}))
        assert doc["x"] == 10
        assert doc["y"] == 20

    # -- setdefault --

    def test_setdefault_missing(self) -> None:
        doc = Document.parse("x = 1\n")
        result = doc.setdefault("y", 42)
        assert result == 42
        assert doc["y"] == 42

    def test_setdefault_existing(self) -> None:
        doc = Document.parse("x = 1\n")
        result = doc.setdefault("x", 99)
        assert result == 1

    def test_setdefault_no_default_existing(self) -> None:
        doc = Document.parse("x = 1\n")
        result = doc.setdefault("x")
        assert result == 1

    def test_setdefault_no_default_missing(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="TOML has no null type"):
            doc.setdefault("y")

    # -- clear --

    def test_clear(self, doc: Document) -> None:
        doc.clear()
        assert len(doc) == 0


# ---------------------------------------------------------------------------
# Live dictionary views (KeysView, ValuesView, ItemsView)
# ---------------------------------------------------------------------------


class TestViews:
    # -- KeysView --

    def test_keys_view_is_live(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        kv = doc.keys()
        assert set(kv) == {"a", "b"}
        doc["c"] = 3
        assert set(kv) == {"a", "b", "c"}

    def test_keys_view_len(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        kv = doc.keys()
        assert len(kv) == 2
        doc["c"] = 3
        assert len(kv) == 3

    def test_keys_view_contains(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        kv = doc.keys()
        assert "a" in kv
        assert "z" not in kv

    def test_keys_view_repr(self) -> None:
        doc = Document.parse("a = 1\n")
        kv = doc.keys()
        assert "KeysView" in repr(kv)
        assert "'a'" in repr(kv)

    def test_keys_view_set_intersection(self) -> None:
        doc = Document.parse("a = 1\nb = 2\nc = 3\n")
        assert doc.keys() & {"a", "c"} == {"a", "c"}

    def test_keys_view_set_union(self) -> None:
        doc = Document.parse("a = 1\n")
        assert doc.keys() | {"b"} == {"a", "b"}

    def test_keys_view_set_difference(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc.keys() - {"b"} == {"a"}

    def test_keys_view_reversed(self) -> None:
        doc = Document.parse("a = 1\nb = 2\nc = 3\n")
        assert list(reversed(doc.keys())) == ["c", "b", "a"]

    # -- ValuesView --

    def test_values_view_is_live(self) -> None:
        doc = Document.parse("a = 1\n")
        vv = doc.values()
        assert len(vv) == 1
        doc["b"] = 2
        assert len(vv) == 2

    def test_values_view_iter(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        vals = list(doc.values())
        assert vals[0] == 1
        assert vals[1] == 2

    def test_values_view_contains(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert 1 in doc.values()
        assert 99 not in doc.values()

    # -- ItemsView --

    def test_items_view_is_live(self) -> None:
        doc = Document.parse("a = 1\n")
        iv = doc.items()
        assert len(iv) == 1
        doc["b"] = 2
        assert len(iv) == 2

    def test_items_view_iter(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        pairs = list(doc.items())
        assert pairs[0][0] == "a"
        assert pairs[0][1] == 1

    def test_items_view_contains(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert ("a", 1) in doc.items()  # type: ignore[comparison-overlap]
        assert ("a", 99) not in doc.items()  # type: ignore[comparison-overlap]
        assert ("z", 1) not in doc.items()  # type: ignore[comparison-overlap]

    # -- Proxy (nested) views --

    def test_proxy_keys_view_is_live(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        kv = doc["t"].keys()
        assert set(kv) == {"a"}
        doc["t"]["b"] = 2
        kv2 = doc["t"].keys()
        assert set(kv2) == {"a", "b"}

    def test_proxy_values_view(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        vals = list(doc["t"].values())
        assert len(vals) == 2

    def test_proxy_items_view(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        pairs = dict(doc["t"].items())
        assert pairs["a"] == 1
        assert pairs["b"] == 2

    # -- ABC registration --

    def test_keys_view_isinstance(self) -> None:
        doc = Document.parse("a = 1\n")
        assert isinstance(doc.keys(), KeysView)

    def test_values_view_isinstance(self) -> None:
        doc = Document.parse("a = 1\n")
        assert isinstance(doc.values(), ValuesView)

    def test_items_view_isinstance(self) -> None:
        doc = Document.parse("a = 1\n")
        assert isinstance(doc.items(), ItemsView)

    def test_document_isinstance_mutable_mapping(self) -> None:
        doc = Document.parse("a = 1\n")
        assert isinstance(doc, MutableMapping)

    # -- KeysView: xor and eq --

    def test_keys_view_set_xor(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc.keys() ^ {"b", "c"} == {"a", "c"}

    def test_keys_view_eq(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc.keys() == {"a", "b"}
        assert doc.keys() != {"a"}

    # -- ValuesView: repr and eq --

    def test_values_view_repr(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        r = repr(doc.values())
        assert "ValuesView" in r
        assert "2 values" in r

    def test_values_view_eq(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        vals = list(doc.values())
        assert doc.values() == vals
        assert doc.values() != [1]
        assert doc.values() != [1, 2, 3]
        assert doc.values() != 42  # non-iterable

    # -- ItemsView: repr and eq --

    def test_items_view_repr(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        r = repr(doc.items())
        assert "ItemsView" in r
        assert "2 items" in r

    def test_items_view_eq(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        pairs = list(doc.items())
        assert doc.items() == pairs
        assert doc.items() != [("a", 1)]
        assert doc.items() != [("a", 1), ("b", 99)]
        assert doc.items() != 42  # non-iterable

    def test_items_view_contains_non_tuple(self) -> None:
        doc = Document.parse("a = 1\n")
        with pytest.raises(TypeError):
            "not a tuple" in doc.items()  # type: ignore[comparison-overlap]  # noqa: B015

    # -- Nested (non-root path) view operations --

    def test_proxy_keys_view_contains(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        kv = doc["t"].keys()
        assert "a" in kv
        assert "z" not in kv

    def test_proxy_values_view_contains(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        assert 1 in doc["t"].values()
        assert 99 not in doc["t"].values()

    def test_proxy_values_view_repr(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        r = repr(doc["t"].values())
        assert "ValuesView" in r

    def test_proxy_items_view_contains(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert ("a", 1) in doc["t"].items()
        assert ("z", 1) not in doc["t"].items()

    def test_proxy_items_view_repr(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        r = repr(doc["t"].items())
        assert "ItemsView" in r

    def test_items_view_contains_wrong_length_tuple(self) -> None:
        doc = Document.parse("a = 1\n")
        assert ("a", 1, "extra") not in doc.items()  # type: ignore[comparison-overlap]

    def test_items_view_eq_other_longer(self) -> None:
        doc = Document.parse("a = 1\n")
        assert doc.items() != [("a", 1), ("b", 2)]

    def test_values_view_eq_element_mismatch(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc.values() != [1, 99]
