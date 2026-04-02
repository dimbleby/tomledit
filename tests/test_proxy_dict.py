"""Tests for Item proxy: dict-like methods."""

from __future__ import annotations

from collections.abc import (
    ItemsView,
    KeysView,
    MutableMapping,
    ValuesView,
)
from datetime import datetime
from types import MappingProxyType

import pytest

import tomledit
from tests.conftest import ItemsMapping, toml_literal
from tomledit import Document

# ---------------------------------------------------------------------------
# Proxy dict-like methods (keys, values, items, get, pop, update, etc.)
# ---------------------------------------------------------------------------


class TestProxyDictMethods:
    # -- keys / values / items --

    def test_keys(self, doc: Document) -> None:
        assert set(doc["owner"].keys()) == {"name", "age", "active"}

    def test_values(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        vals = doc["t"].values()
        assert len(vals) == 2

    def test_values_are_live_proxies(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            [t.inner]
            val = 10
        """)
        )
        vals = list(doc["t"].values())
        vals[0]["val"] = 99
        assert doc["t"]["inner"]["val"] == 99

    def test_items(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        pairs = doc["t"].items()
        keys = [k for k, v in pairs]
        assert set(keys) == {"a", "b"}

    def test_items_returns_live_proxies(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            [t.inner]
            val = 10
        """)
        )
        for key, proxy in doc["t"].items():
            if key == "inner":
                proxy["val"] = 99
        assert doc["t"]["inner"]["val"] == 99

    # -- get --

    def test_get_returns_live_proxy(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            [t.inner]
            val = 10
        """)
        )
        inner = doc["t"].get("inner")
        assert inner is not None
        inner["val"] = 42
        assert doc["t"]["inner"]["val"] == 42

    def test_get_existing(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [tbl]
            x = 1
        """)
        )
        assert doc["tbl"].get("x") == 1

    def test_get_missing_no_default(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [tbl]
            x = 1
        """)
        )
        assert doc["tbl"].get("y") is None

    def test_get_missing_with_default(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [tbl]
            x = 1
        """)
        )
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

    def test_pop_existing_ignores_default(self, doc: Document) -> None:
        val = doc["owner"].pop("age", 99)
        assert val == 30
        assert "age" not in doc["owner"]

    def test_pop_too_many_args(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [owner]
            name = 'Tom'
        """)
        )
        with pytest.raises(TypeError, match="at most 2 arguments"):
            doc["owner"].pop("name", 1, 2)

    def test_pop_by_key_returns_native(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
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

    def test_update_list_pair_mapping(self, doc: Document) -> None:
        data: dict[str, object] = {"name": "Bob", "email": "bob@example.com"}
        doc["owner"].update(ItemsMapping(data, [[k, v] for k, v in data.items()]))
        assert doc["owner"]["name"] == "Bob"
        assert doc["owner"]["email"] == "bob@example.com"

    def test_update_self(self, doc: Document) -> None:
        """DictItem.update(self) must not panic (self-referencing)."""
        owner = doc["owner"]
        owner.update(owner)
        assert doc["owner"]["name"] == "Alice"

    def test_ior_self(self, doc: Document) -> None:
        """DictItem |= self must not panic (self-referencing)."""
        owner = doc["owner"]
        owner |= owner
        assert doc["owner"]["name"] == "Alice"

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

    def test_inline_table_pop_missing_raises(self) -> None:
        doc = Document.parse("meta = {x = 1}\n")
        with pytest.raises(KeyError):
            doc["meta"].pop("nonexistent")

    def test_inline_table_pop_missing_default(self) -> None:
        doc = Document.parse("meta = {x = 1}\n")
        assert doc["meta"].pop("nonexistent", 42) == 42

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
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
        """)
        )
        del doc["x"]
        assert "x" not in doc
        assert "y" in doc

    def test_delitem_raises_key_error(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(KeyError):
            del doc["nonexistent"]

    # -- pop argument handling --

    def test_pop_existing(self) -> None:
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 2
        """)
        )
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
            doc.pop("x", 1, 2)  # type: ignore[call-overload]  # ty: ignore[no-matching-overload]

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
        doc = Document.parse(
            toml_literal("""
            [a]
            [a.b]
            x = 1
        """)
        )
        assert doc.pop("a") == {"b": {"x": 1}}

    # -- items / values return live proxies --

    def test_items_returns_live_proxies(self, doc: Document) -> None:
        for key, proxy in doc.items():
            if key == "owner":
                proxy["name"] = "Charlie"
                break
        assert doc["owner"]["name"] == "Charlie"

    def test_values_returns_live_proxies(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [section]
            val = 10
        """)
        )
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

    def test_update_list_pair_mapping(self) -> None:
        doc = Document.parse("x = 1\n")
        data: dict[str, object] = {"x": 10, "y": 20}
        doc.update(ItemsMapping(data, [[k, v] for k, v in data.items()]))  # type: ignore[call-overload]  # ty: ignore[no-matching-overload]
        assert doc["x"] == 10
        assert doc["y"] == 20

    def test_update_mapping_with_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\nx = 1\n')
        doc.update({doc["key"]: 2})  # type: ignore[dict-item]  # ty: ignore[no-matching-overload]
        assert doc["x"] == 2

    def test_update_iterable_of_pairs_with_proxy_key(self) -> None:
        doc = Document.parse('key = "x"\nx = 1\n')
        doc.update([(doc["key"], 2)])  # type: ignore[list-item]  # ty: ignore[no-matching-overload]
        assert doc["x"] == 2

    def test_update_rejects_non_string_dict_key(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="keys must be strings"):
            doc.update({1: 2})  # type: ignore[dict-item]  # ty: ignore[no-matching-overload]

    def test_update_rejects_non_string_pair_key(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="keys must be strings"):
            doc.update(ItemsMapping({"unused": 0}, [[1, 2]]))  # type: ignore[call-overload]  # ty: ignore[no-matching-overload]

    @pytest.mark.parametrize(
        "other",
        [
            [[]],
            [["x"]],
            [["x", 10, 20]],
        ],
        ids=["empty", "short", "long"],
    )
    def test_update_rejects_non_pair_iterables(
        self,
        other: list[list[object]],
    ) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(ValueError, match="expected a length-2 iterable pair"):
            doc.update(other)  # type: ignore[arg-type]  # ty: ignore[no-matching-overload]

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
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        kv = doc.keys()
        assert set(kv) == {"a", "b"}
        doc["c"] = 3
        assert set(kv) == {"a", "b", "c"}

    def test_keys_view_len(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        kv = doc.keys()
        assert len(kv) == 2
        doc["c"] = 3
        assert len(kv) == 3

    def test_keys_view_contains(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        kv = doc.keys()
        assert "a" in kv
        assert "z" not in kv

    def test_keys_view_repr(self) -> None:
        doc = Document.parse("a = 1\n")
        kv = doc.keys()
        assert "KeysView" in repr(kv)
        assert "'a'" in repr(kv)

    def test_keys_view_set_intersection(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
            c = 3
        """)
        )
        assert doc.keys() & {"a", "c"} == {"a", "c"}

    def test_keys_view_set_intersection_with_proxy_key(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        assert doc.keys() & [doc["key"]] == {"a"}

    def test_keys_view_set_union(self) -> None:
        doc = Document.parse("a = 1\n")
        assert doc.keys() | {"b"} == {"a", "b"}

    def test_keys_view_set_difference(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        assert doc.keys() - {"b"} == {"a"}

    def test_keys_view_set_difference_with_proxy_key(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        assert doc.keys() - [doc["key"]] == {"key"}

    def test_keys_view_reversed(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
            c = 3
        """)
        )
        assert list(reversed(doc.keys())) == ["c", "b", "a"]

    def test_values_view_reversed(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
            c = 3
        """)
        )
        vals = [v.value for v in reversed(doc.values())]
        assert vals == [3, 2, 1]

    def test_items_view_reversed(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
            c = 3
        """)
        )
        pairs = [(k, v.value) for k, v in reversed(doc.items())]
        assert pairs == [("c", 3), ("b", 2), ("a", 1)]

    def test_proxy_values_view_reversed(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        vals = [v.value for v in reversed(doc["t"].values())]
        assert vals == [2, 1]

    def test_proxy_items_view_reversed(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        pairs = [(k, v.value) for k, v in reversed(doc["t"].items())]
        assert pairs == [("b", 2), ("a", 1)]

    # -- ValuesView --

    def test_values_view_is_live(self) -> None:
        doc = Document.parse("a = 1\n")
        vv = doc.values()
        assert len(vv) == 1
        doc["b"] = 2
        assert len(vv) == 2

    def test_values_view_iter(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        vals = list(doc.values())
        assert vals[0] == 1
        assert vals[1] == 2

    def test_values_view_contains(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
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
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        pairs = list(doc.items())
        assert pairs[0][0] == "a"
        assert pairs[0][1] == 1

    def test_items_view_contains(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        assert ("a", 1) in doc.items()  # type: ignore[comparison-overlap]
        assert ("a", 99) not in doc.items()  # type: ignore[comparison-overlap]
        assert ("z", 1) not in doc.items()  # type: ignore[comparison-overlap]

    # -- Proxy (nested) views --

    def test_proxy_keys_view_is_live(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        kv = doc["t"].keys()
        assert set(kv) == {"a"}
        doc["t"]["b"] = 2
        kv2 = doc["t"].keys()
        assert set(kv2) == {"a", "b"}

    def test_proxy_values_view(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        vals = list(doc["t"].values())
        assert len(vals) == 2

    def test_proxy_items_view(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
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
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        assert doc.keys() ^ {"b", "c"} == {"a", "c"}

    def test_keys_view_union_with_list(self) -> None:
        """dict_keys.__or__ accepts any iterable, not just sets."""
        doc = Document.parse("a = 1\n")
        assert doc.keys() | ["b", "c"] == {"a", "b", "c"}

    def test_keys_view_xor_with_list(self) -> None:
        """dict_keys.__xor__ accepts any iterable, not just sets."""
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc.keys() ^ ["b", "c"] == {"a", "c"}

    def test_keys_view_eq(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        assert doc.keys() == {"a", "b"}
        assert doc.keys() != {"a"}

    # -- ValuesView: repr and eq --

    def test_values_view_repr(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        r = repr(doc.values())
        assert "ValuesView" in r
        assert "2 values" in r

    def test_values_view_eq(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        # Python's dict_values has no equality support — falls back to
        # identity comparison.  So two distinct views are never equal.
        v = doc.values()
        assert v == v  # identity
        assert doc.values() != doc.values()  # distinct objects
        assert doc.values() != list(doc.values())  # type: ignore[comparison-overlap]
        assert doc.values() != [1, 99]  # type: ignore[comparison-overlap]
        assert doc.values() != 42  # type: ignore[comparison-overlap]  # non-iterable

    # -- ItemsView: repr and eq --

    def test_items_view_repr(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        r = repr(doc.items())
        assert "ItemsView" in r
        assert "2 items" in r

    def test_items_view_eq(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1
            b = 2
        """)
        )
        # ItemsView uses set semantics, like Python's dict_items.
        assert doc.items() == {("a", 1), ("b", 2)}
        assert doc.items() != {("a", 1)}
        assert doc.items() != {("a", 1), ("b", 99)}
        assert doc.items() != 42  # non-iterable

    # -- Nested (non-root path) view operations --

    def test_proxy_keys_view_contains(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        kv = doc["t"].keys()
        assert "a" in kv
        assert "z" not in kv

    def test_proxy_values_view_contains(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        assert 1 in doc["t"].values()
        assert 99 not in doc["t"].values()

    def test_proxy_values_view_repr(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        r = repr(doc["t"].values())
        assert "ValuesView" in r

    def test_proxy_items_view_contains(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        assert ("a", 1) in doc["t"].items()
        assert ("z", 1) not in doc["t"].items()

    def test_proxy_items_view_repr(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        r = repr(doc["t"].items())
        assert "ItemsView" in r

    def test_items_view_contains_wrong_length_tuple(self) -> None:
        doc = Document.parse("a = 1\n")
        assert ("a", 1, "extra") not in doc.items()  # type: ignore[comparison-overlap]

    def test_items_view_eq_other_longer(self) -> None:
        doc = Document.parse("a = 1\n")
        assert doc.items() != [("a", 1), ("b", 2)]


class TestItemsViewSetEquality:
    """ItemsView.__eq__ should be set-like (order independent), per Python spec."""

    def test_items_view_eq_different_order(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        # Same pairs as a set — order should not matter.
        assert doc.items() == {("b", 2), ("a", 1)}

    def test_items_view_eq_same_order(self) -> None:
        """Sanity: same pairs as a set should definitely be equal."""
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc.items() == {("a", 1), ("b", 2)}

    def test_items_view_ne_list(self) -> None:
        """Items view should NOT equal a list (just like Python's dict_items)."""
        doc = Document.parse("a = 1\nb = 2\n")
        assert doc.items() != [("a", 1), ("b", 2)]


class TestKeysViewContainsNonString:
    """KeysView.__contains__ should return False for non-string keys, not TypeError."""

    def test_int_key(self) -> None:
        doc = Document.parse("a = 1\n")
        assert 1 not in doc.keys()  # type: ignore[comparison-overlap]  # noqa: SIM118

    def test_none_key(self) -> None:
        doc = Document.parse("a = 1\n")
        assert None not in doc.keys()  # noqa: SIM118

    def test_list_key(self) -> None:
        doc = Document.parse("a = 1\n")
        assert [1, 2] not in doc.keys()  # type: ignore[comparison-overlap]  # noqa: SIM118

    def test_nested_keys_view(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert 42 not in doc["t"].keys()  # noqa: SIM118


class TestItemsViewContainsEdgeCases:
    """ItemsView.__contains__ should return False for non-tuples and
    tuples with non-string first element, not raise TypeError."""

    def test_non_tuple(self) -> None:
        doc = Document.parse("a = 1\n")
        assert 1 not in doc.items()  # type: ignore[comparison-overlap]

    def test_list_pair(self) -> None:
        doc = Document.parse("a = 1\n")
        assert ["a", 1] not in doc.items()  # type: ignore[comparison-overlap]

    def test_int_key_in_tuple(self) -> None:
        doc = Document.parse("a = 1\n")
        assert (1, "a") not in doc.items()  # type: ignore[comparison-overlap]

    def test_none_key_in_tuple(self) -> None:
        doc = Document.parse("a = 1\n")
        assert (None, 1) not in doc.items()  # type: ignore[comparison-overlap]

    def test_three_element_tuple(self) -> None:
        """3-tuples are not valid (key, value) pairs — should be False."""
        doc = Document.parse("a = 1\n")
        assert ("a", 1, "extra") not in doc.items()  # type: ignore[comparison-overlap]


class TestItemsViewEqUnhashable:
    """ItemsView.__eq__ should work even when values are unhashable (lists, dicts)."""

    def test_eq_with_list_values(self) -> None:
        doc = Document.parse("a = [1, 2, 3]\n")
        assert doc.items() == doc.items()

    def test_eq_with_dict_values(self) -> None:
        doc = Document.parse("[a]\nx = 1\n")
        assert doc.items() == doc.items()

    def test_eq_cross_document(self) -> None:
        doc1 = Document.parse("a = [1, 2]\nb = [3, 4]\n")
        doc2 = Document.parse("b = [3, 4]\na = [1, 2]\n")
        assert doc1.items() == doc2.items()

    def test_ne_with_list_values(self) -> None:
        doc1 = Document.parse("a = [1, 2]\n")
        doc2 = Document.parse("a = [1, 3]\n")
        assert doc1.items() != doc2.items()


class TestContainsNonStringKey:
    """__contains__ should return False for non-string keys on tables/Document,
    matching Python dict behavior (no TypeError)."""

    @pytest.mark.parametrize("key", [42, 3.14, True, None, [1, 2], {"x": 1}])
    def test_document_contains_non_string(self, key: object) -> None:
        doc = Document.parse("a = 1\n")
        assert key not in doc

    @pytest.mark.parametrize("key", [42, 3.14, True, None, [1, 2], {"x": 1}])
    def test_dict_item_contains_non_string(self, key: object) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert key not in doc["t"]

    @pytest.mark.parametrize("key", [42, 3.14, True, None, [1, 2], {"x": 1}])
    def test_inline_table_contains_non_string(self, key: object) -> None:
        doc = Document.parse("t = {a = 1}\n")
        assert key not in doc["t"]


class TestGetNonStringKey:
    """get() should return the default for non-string keys on tables/Document,
    matching Python dict behavior (no TypeError)."""

    @pytest.mark.parametrize("key", [42, 3.14, True, None])
    def test_document_get_non_string_returns_none(self, key: object) -> None:
        doc = Document.parse("a = 1\n")
        assert doc.get(key) is None  # type: ignore[call-overload]  # ty: ignore[invalid-argument-type]

    @pytest.mark.parametrize("key", [42, 3.14, True, None])
    def test_document_get_non_string_returns_default(self, key: object) -> None:
        doc = Document.parse("a = 1\n")
        assert doc.get(key, "fallback") == "fallback"  # type: ignore[call-overload]  # ty: ignore[invalid-argument-type]

    @pytest.mark.parametrize("key", [42, 3.14, True, None])
    def test_dict_item_get_non_string_returns_none(self, key: object) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert doc["t"].get(key) is None

    @pytest.mark.parametrize("key", [42, 3.14, True, None])
    def test_dict_item_get_non_string_returns_default(self, key: object) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert doc["t"].get(key, "fallback") == "fallback"


class TestPopitem:
    """popitem() removes and returns the last (key, value) pair."""

    def test_document_popitem(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        key, val = doc.popitem()
        assert key == "b"
        assert val == 2
        assert list(doc) == ["a"]

    def test_document_popitem_empty_raises(self) -> None:
        doc = Document()
        with pytest.raises(KeyError, match="dictionary is empty"):
            doc.popitem()

    def test_dict_item_popitem(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        key, val = doc["t"].popitem()
        assert key == "b"
        assert val == 2
        assert list(doc["t"]) == ["a"]

    def test_dict_item_popitem_empty_raises(self) -> None:
        doc = Document.parse("[t]\n")
        with pytest.raises(KeyError, match="dictionary is empty"):
            doc["t"].popitem()

    def test_inline_table_popitem(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        key, val = doc["t"].popitem()
        assert key == "b"
        assert val == 2
        assert list(doc["t"]) == ["a"]

    def test_inline_table_popitem_preserves_comments(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2, c = 3}\n")
        doc["t"]["a"].inline_comment = "# keep me"
        doc["t"].popitem()  # removes "c"
        assert doc["t"]["a"].inline_comment == "# keep me"


class TestMergeOperators:
    """| and |= operators (PEP 584)."""

    # -- Document | other --

    def test_document_or_dict(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        result = doc | {"b": 3, "c": 4}
        assert isinstance(result, Document)
        assert result.as_toml() == toml_literal("""
            a = 1
            b = 3
            c = 4
        """)
        assert doc.as_toml() == "a = 1\nb = 2\n"  # original unchanged

    def test_document_or_document(self) -> None:
        d1 = Document.parse("# on a\na = 1\n")
        d2 = Document.parse("# on b\nb = 2\n")
        result = d1 | d2
        assert isinstance(result, Document)
        assert result.as_toml() == toml_literal("""
            # on a
            a = 1
            # on b
            b = 2
        """)

    def test_document_or_list_pair_mapping(self) -> None:
        doc = Document.parse("a = 1\n")
        result = doc | ItemsMapping({"b": 2}, [["b", 2]])  # type: ignore[operator]  # ty: ignore[unsupported-operator]
        assert isinstance(result, Document)
        assert result.as_toml() == toml_literal("""
            a = 1
            b = 2
        """)

    def test_document_or_override_keeps_lhs_comment(self) -> None:
        """When RHS overrides a key, the LHS comment on that key is kept."""
        doc = Document.parse("# important\na = 1\n")
        result = doc | {"a": 99}
        assert result.as_toml() == toml_literal("""
            # important
            a = 99
        """)

    # -- other | Document (__ror__ returns plain dict) --

    def test_dict_or_document(self) -> None:
        doc = Document.parse("b = 2\n")
        result = {"a": 1} | doc
        assert isinstance(result, dict)
        assert result == {"a": 1, "b": 2}

    def test_dict_or_document_override(self) -> None:
        doc = Document.parse("a = 99\nb = 2\n")
        result = {"a": 1} | doc
        assert result == {"a": 99, "b": 2}

    # -- Document |= other --

    def test_document_ior(self) -> None:
        doc = Document.parse("# on a\na = 1\n")
        doc |= {"b": 2, "c": 3}
        assert isinstance(doc, Document)
        assert doc.as_toml() == toml_literal("""
            # on a
            a = 1
            b = 2
            c = 3
        """)

    def test_document_ior_override(self) -> None:
        doc = Document.parse("# keep this\na = 1\nb = 2\n")
        doc |= {"b": 3}
        assert doc.as_toml() == toml_literal("""
            # keep this
            a = 1
            b = 3
        """)

    # -- DictItem | other --

    def test_dict_item_or_dict(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        result = doc["t"] | {"b": 2}
        assert isinstance(result, tomledit.DictItem)
        assert result.value == {"a": 1, "b": 2}
        assert doc["t"].value == {"a": 1}  # original unchanged

    def test_dict_item_or_dict_item(self) -> None:
        d1 = Document.parse("[x]\n# on a\na = 1\n")
        d2 = Document.parse("[y]\n# on b\nb = 2  # inline\n")
        result = d1["x"] | d2["y"]
        assert isinstance(result, tomledit.DictItem)
        assert result["a"].comment == "# on a"
        assert result["b"].comment == "# on b"
        assert result["b"].inline_comment == "# inline"
        assert result.value == {"a": 1, "b": 2}

    def test_dict_item_or_list_pair_mapping(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        result = doc["t"] | ItemsMapping({"b": 2}, [["b", 2]])
        assert isinstance(result, tomledit.DictItem)
        assert result.value == {"a": 1, "b": 2}

    # -- other | DictItem (__ror__ returns plain dict) --

    def test_dict_or_dict_item(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        result = {"b": 2} | doc["t"]
        assert isinstance(result, dict)
        assert result == {"a": 1, "b": 2}

    def test_ror_preserves_non_toml_values_document(self) -> None:
        """LHS values that are not TOML-compatible (e.g. None) pass through."""
        doc = Document({"b": 1})
        result = {"a": None, "c": (1, 2)} | doc
        assert isinstance(result, dict)
        assert result["a"] is None
        assert result["c"] == (1, 2)
        assert result["b"] == 1

    def test_ror_non_dict_mapping_document(self) -> None:
        """LHS is a non-dict Mapping (copy_mapping_to_pydict fallback)."""
        # ItemsMapping is not a dict subclass, so PyDict::cast fails and
        # the items()-based fallback is exercised.
        doc = Document({"b": 2})
        result = ItemsMapping({"a": None, "b": 1}) | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]
        assert isinstance(result, dict)
        assert result["a"] is None
        assert result["b"] == 2

    def test_ror_non_dict_mapping_dict_item(self) -> None:
        """LHS is a non-dict Mapping against a DictItem."""
        doc = Document.parse("[t]\nb = 2\n")
        result = ItemsMapping({"a": None, "b": 1}) | doc["t"]
        assert isinstance(result, dict)
        assert result["a"] is None
        assert result["b"] == 2

    def test_ror_dict_with_proxy_key_document(self) -> None:
        base = Document.parse('key = "name"\n')
        doc = Document({"x": 1})
        result = {base["key"]: 2} | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]
        assert [type(key) for key in result] == [str, str]
        assert list(result.keys()) == ["name", "x"]
        assert result["name"] == 2
        assert result["x"] == 1

    def test_ror_dict_with_proxy_key_dict_item(self) -> None:
        base = Document.parse('key = "name"\n')
        doc = Document.parse("[t]\nx = 1\n")
        result = {base["key"]: 2} | doc["t"]
        assert [type(key) for key in result] == [str, str]
        assert list(result.keys()) == ["name", "x"]
        assert result["name"] == 2
        assert result["x"] == 1

    def test_ror_non_dict_mapping_with_proxy_key_document(self) -> None:
        base = Document.parse('key = "name"\n')
        doc = Document({"x": 1})
        result = ItemsMapping({"unused": 0}, [[base["key"], 2]]) | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]
        assert [type(key) for key in result] == [str, str]
        assert list(result.keys()) == ["name", "x"]
        assert result["name"] == 2
        assert result["x"] == 1

    def test_ror_non_dict_mapping_with_proxy_key_dict_item(self) -> None:
        base = Document.parse('key = "name"\n')
        doc = Document.parse("[t]\nx = 1\n")
        result = ItemsMapping({"unused": 0}, [[base["key"], 2]]) | doc["t"]
        assert [type(key) for key in result] == [str, str]
        assert list(result.keys()) == ["name", "x"]
        assert result["name"] == 2
        assert result["x"] == 1

    def test_ror_non_dict_mapping_with_stale_proxy_key_document(self) -> None:
        base = Document.parse('key = "name"\n')
        key = base["key"]
        del base["key"]
        doc = Document({"x": 1})
        with pytest.raises(RuntimeError, match="stale"):
            ItemsMapping({"unused": 0}, [[key, 2]]) | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]

    def test_ror_non_dict_mapping_with_stale_proxy_key_dict_item(self) -> None:
        base = Document.parse('key = "name"\n')
        key = base["key"]
        del base["key"]
        doc = Document.parse("[t]\nx = 1\n")
        with pytest.raises(RuntimeError, match="stale"):
            ItemsMapping({"unused": 0}, [[key, 2]]) | doc["t"]

    def test_ror_non_dict_mapping_rejects_malformed_pair_document(self) -> None:
        doc = Document({"x": 1})
        with pytest.raises(ValueError, match="expected a length-2 iterable pair"):
            ItemsMapping({"unused": 0}, [[]]) | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]

    def test_ror_non_dict_mapping_rejects_malformed_pair_dict_item(self) -> None:
        doc = Document.parse("[t]\nx = 1\n")
        with pytest.raises(ValueError, match="expected a length-2 iterable pair"):
            ItemsMapping({"unused": 0}, [[]]) | doc["t"]

    def test_ror_dict_preserves_non_string_key(self) -> None:
        doc = Document({"x": 1})
        result = {1: "one"} | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]
        assert [type(key) for key in result] == [int, str]
        assert result[1] == "one"
        assert result["x"] == 1

    def test_ror_non_dict_mapping_preserves_non_string_key(self) -> None:
        doc = Document.parse("[t]\nx = 1\n")
        result = ItemsMapping({"unused": 0}, [[1, "one"]]) | doc["t"]
        assert [type(key) for key in result] == [int, str]
        assert result[1] == "one"
        assert result["x"] == 1

    def test_ror_list_pair_mapping_document(self) -> None:
        """items() may yield pair-like sequences, not only tuples."""
        data: dict[str, object] = {"a": None, "b": 1}
        doc = Document({"b": 2})
        result = ItemsMapping(data, [[k, v] for k, v in data.items()]) | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]
        assert isinstance(result, dict)
        assert result["a"] is None
        assert result["b"] == 2

    def test_ror_list_pair_mapping_dict_item(self) -> None:
        """DictItem __ror__ accepts mappings whose items() yields lists."""
        data: dict[str, object] = {"a": None, "b": 1}
        doc = Document.parse("[t]\nb = 2\n")
        result = ItemsMapping(data, [[k, v] for k, v in data.items()]) | doc["t"]
        assert isinstance(result, dict)
        assert result["a"] is None
        assert result["b"] == 2

    def test_ror_preserves_non_toml_values_dict_item(self) -> None:
        """LHS values that are not TOML-compatible (e.g. None) pass through."""
        doc = Document.parse("[t]\nb = 1\n")
        result = {"a": None} | doc["t"]
        assert isinstance(result, dict)
        assert result["a"] is None
        assert result["b"] == 1

    def test_or_non_mapping_returns_not_implemented(self) -> None:
        """| with a non-mapping type falls through to TypeError."""
        doc = Document({"a": 1})
        with pytest.raises(TypeError, match="unsupported operand"):
            doc | [1, 2]  # type: ignore[operator]  # ty: ignore[unsupported-operator]

    def test_dict_item_or_non_mapping_returns_not_implemented(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="unsupported operand"):
            doc["t"] | [1, 2]

    # -- DictItem |= other --

    def test_dict_item_ior(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        t = doc["t"]
        t |= {"b": 2}
        assert doc.as_toml() == toml_literal("""
            [t]
            a = 1
            b = 2
        """)

    # -- InlineTable --

    def test_inline_table_or(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        result = doc["t"] | {"b": 2}
        assert isinstance(result, tomledit.DictItem)
        assert result.value == {"a": 1, "b": 2}

    # -- Empty cases --

    def test_document_or_empty(self) -> None:
        doc = Document.parse("a = 1\n")
        result = doc | {}
        assert result.as_toml() == "a = 1\n"

    def test_empty_document_or_dict(self) -> None:
        doc = Document()
        result = doc | {"a": 1}
        assert isinstance(result, Document)
        assert result.value == {"a": 1}

    # -- |= with iterables of pairs --

    def test_document_ior_list_of_pairs(self) -> None:
        doc = Document.parse("a = 1\n")
        doc |= [("b", 2), ("c", 3)]
        assert doc.as_toml() == toml_literal("""
            a = 1
            b = 2
            c = 3
        """)

    def test_dict_item_ior_list_of_pairs(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        doc["t"] |= [("b", 2)]
        assert doc.as_toml() == toml_literal("""
            [t]
            a = 1
            b = 2
        """)

    def test_document_ior_generator(self) -> None:
        doc = Document.parse("a = 1\n")
        doc |= ((k, v) for k, v in [("b", 2), ("c", 3)])
        assert doc.as_toml() == toml_literal("""
            a = 1
            b = 2
            c = 3
        """)

    # -- Inline table merges (toml_edit-level) --

    def test_inline_table_or_inline_table(self) -> None:
        doc1 = Document.parse("t = {a = 1}\n")
        doc2 = Document.parse("u = {b = 2}\n")
        result = doc1["t"] | doc2["u"]
        assert isinstance(result, tomledit.DictItem)
        assert result.value == {"a": 1, "b": 2}

    def test_inline_table_ior_inline_table(self) -> None:
        doc1 = Document.parse("t = {a = 1}\n")
        doc2 = Document.parse("u = {b = 2, c = 3}\n")
        doc1["t"] |= doc2["u"]
        assert doc1.as_toml() == "t = {a = 1, b = 2, c = 3}\n"

    def test_inline_table_or_replaces_existing_key(self) -> None:
        doc1 = Document.parse("t = {a = 1, b = 2}\n")
        doc2 = Document.parse("u = {b = 99}\n")
        result = doc1["t"] | doc2["u"]
        assert result.value == {"a": 1, "b": 99}

    def test_inline_table_or_preserves_spacing(self) -> None:
        doc1 = Document.parse("t = {a = 1}\n")
        doc2 = Document.parse("u = {b = 2, c = 3}\n")
        result = doc1["t"] | doc2["u"]
        assert result.as_toml() == "{a = 1, b = 2, c = 3}"

    # -- NotImplemented / TypeError for non-mappings --

    def test_document_or_non_mapping_raises(self) -> None:
        doc = Document.parse("a = 1\n")
        with pytest.raises(TypeError, match="unsupported operand"):
            doc | 42  # type: ignore[operator]  # ty: ignore[unsupported-operator]

    def test_document_ror_non_mapping_raises(self) -> None:
        doc = Document.parse("a = 1\n")
        with pytest.raises(TypeError, match="unsupported operand"):
            42 | doc  # type: ignore[operator]  # ty: ignore[unsupported-operator]

    def test_dict_item_or_non_mapping_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="unsupported operand"):
            doc["t"] | 42

    def test_dict_item_ror_non_mapping_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="unsupported operand"):
            42 | doc["t"]

    # -- |= replaces existing keys --

    def test_dict_item_ior_replaces_existing(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        t = doc["t"]
        t |= {"b": 99}
        assert doc.as_toml() == toml_literal("""
            [t]
            a = 1
            b = 99
        """)

    # -- |= edge cases --

    def test_document_ior_self_assign(self) -> None:
        doc = Document.parse("a = 1\n")
        doc |= doc
        assert doc.as_toml() == "a = 1\n"

    def test_dict_item_ior_with_own_document(self) -> None:
        doc = Document.parse("[section]\na = 1\nb = 2\n")
        doc["section"] |= doc
        assert doc["section"]["a"] == 1

    def test_dict_item_ior_same_document_different_paths(self) -> None:
        doc = Document.parse("[a]\nx = 1\n[b]\ny = 2\n")
        doc["a"] |= doc["b"]
        assert doc["a"]["y"] == 2

    def test_document_ior_dict_with_proxy_values(self) -> None:
        doc = Document.parse("x = 1\ny = 2\n")
        doc |= {"z": doc["x"]}
        assert doc["z"] == 1

    def test_dict_item_ior_dict_with_proxy_values(self) -> None:
        doc = Document.parse("[s]\nx = 1\ny = 2\n")
        doc["s"] |= {"z": doc["s"]["x"]}
        assert doc["s"]["z"] == 1


class TestViewContainsProxy:
    """ValuesView/ItemsView.__contains__ must accept proxy values.

    The views yield proxies from __iter__, so __contains__ must recognise
    those same proxies — otherwise ``x in view`` fails for the very
    objects the view itself produces.
    """

    def test_values_contains_string_proxy(self) -> None:
        doc = Document.parse('name = "Alice"\n')
        assert doc["name"] in doc.values()

    def test_values_contains_bool_proxy(self) -> None:
        doc = Document.parse("flag = true\n")
        assert doc["flag"] in doc.values()

    def test_values_contains_int_proxy(self) -> None:
        doc = Document.parse("num = 42\n")
        assert doc["num"] in doc.values()

    def test_values_contains_table_proxy(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert doc["t"] in doc.values()

    def test_values_contains_array_proxy(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        assert doc["arr"] in doc.values()

    def test_items_contains_string_proxy(self) -> None:
        doc = Document.parse('name = "Alice"\n')
        assert ("name", doc["name"]) in doc.items()

    def test_items_contains_bool_proxy(self) -> None:
        doc = Document.parse("flag = true\n")
        assert ("flag", doc["flag"]) in doc.items()

    def test_items_contains_table_proxy(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        assert ("t", doc["t"]) in doc.items()

    def test_items_contains_string_proxy_key(self) -> None:
        doc = Document.parse('key = "name"\nname = "Alice"\n')
        assert (doc["key"], "Alice") in doc.items()  # type: ignore[comparison-overlap]

    def test_iter_values_roundtrip(self) -> None:
        """Every value from iter(values()) must be found by __contains__."""
        doc = Document.parse('a = 1\nb = "hi"\nc = true\n')
        for v in doc.values():
            assert v in doc.values()

    def test_iter_items_roundtrip(self) -> None:
        """Every pair from iter(items()) must be found by __contains__."""
        doc = Document.parse('a = 1\nb = "hi"\nc = true\n')
        for pair in doc.items():
            assert pair in doc.items()

    def test_dictitem_values_contains_proxy(self) -> None:
        doc = Document.parse('[t]\nname = "Bob"\n')
        assert doc["t"]["name"] in doc["t"].values()

    def test_dictitem_items_contains_proxy(self) -> None:
        doc = Document.parse('[t]\nname = "Bob"\n')
        assert ("name", doc["t"]["name"]) in doc["t"].items()

    def test_dictitem_items_contains_string_proxy_key(self) -> None:
        doc = Document.parse('[t]\nkey = "name"\nname = "Bob"\n')
        assert (doc["t"]["key"], "Bob") in doc["t"].items()


class TestProxyKeyContainment:
    """Proxy string values should work as keys in 'in' checks."""

    def test_proxy_key_in_document(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        assert doc["key"] in doc

    def test_proxy_key_in_dict_proxy(self) -> None:
        doc = Document.parse('[s]\nname = "age"\nage = 30\n')
        assert doc["s"]["name"] in doc["s"]

    def test_proxy_key_in_keys_view(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        assert doc["key"] in doc.keys()  # noqa: SIM118

    def test_proxy_key_in_dict_proxy_keys_view(self) -> None:
        doc = Document.parse('[s]\nname = "age"\nage = 30\n')
        assert doc["s"]["name"] in doc["s"].keys()  # noqa: SIM118

    def test_non_string_proxy_not_in_table(self) -> None:
        """A proxy wrapping a non-string value is not a valid key."""
        doc = Document.parse("port = 8080\na = 1\n")
        assert doc["port"] not in doc


class TestProxyKeyLookup:
    """Proxy string values should work as keys in get/getitem/setdefault."""

    def test_document_getitem_with_proxy_key(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        assert doc[doc["key"]] == 1  # type: ignore[index] # ty: ignore[invalid-argument-type]

    def test_document_get_with_proxy_key(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        assert doc.get(doc["key"]) == 1  # type: ignore[call-overload] # ty: ignore[invalid-argument-type]

    def test_document_get_missing_proxy_key(self) -> None:
        doc = Document.parse('key = "missing"\na = 1\n')
        assert doc.get(doc["key"]) is None  # type: ignore[call-overload] # ty: ignore[invalid-argument-type]

    def test_document_setdefault_with_proxy_key(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        result = doc.setdefault(doc["key"], 99)  # type: ignore[arg-type] # ty: ignore[invalid-argument-type]
        assert result == 1  # existing value, not default

    def test_dict_proxy_getitem_with_proxy_key(self) -> None:
        doc = Document.parse('[s]\nname = "age"\nage = 30\n')
        assert doc["s"][doc["s"]["name"]] == 30  # type: ignore[call-overload] # ty: ignore[invalid-argument-type]

    def test_dict_proxy_get_with_proxy_key(self) -> None:
        doc = Document.parse('[s]\nname = "age"\nage = 30\n')
        assert doc["s"].get(doc["s"]["name"]) == 30

    def test_document_setitem_with_proxy_key(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        doc[doc["key"]] = 99  # type: ignore[index] # ty: ignore[invalid-assignment]
        assert doc["a"] == 99

    def test_document_delitem_with_proxy_key(self) -> None:
        doc = Document.parse('key = "a"\na = 1\n')
        del doc[doc["key"]]  # type: ignore[arg-type] # ty: ignore[invalid-argument-type]
        assert "a" not in doc


class TestImplicitProperty:
    """Tests for the DictItem.implicit property."""

    def test_default_is_not_implicit(self) -> None:
        doc = Document({"tbl": {"x": 1}})
        assert doc["tbl"].implicit is False

    def test_set_implicit_suppresses_header(self) -> None:
        doc = Document()
        doc["a"] = {"b": {"x": 1}}
        assert doc.as_toml() == "[a]\n\n[a.b]\nx = 1\n"
        doc["a"].implicit = True
        assert doc.as_toml() == "[a.b]\nx = 1\n"

    def test_set_implicit_false_restores_header(self) -> None:
        doc = Document.parse("[a.b]\nx = 1\n")
        assert doc["a"].implicit is True
        doc["a"].implicit = False
        assert "[a]" in doc.as_toml()

    def test_implicit_no_op_on_inline_table(self) -> None:
        doc = Document.parse("a = {x = 1}")
        assert doc["a"].implicit is False
        doc["a"].implicit = True  # silently ignored
        assert doc["a"].implicit is False

    def test_cleared_table_roundtrips(self) -> None:
        doc = Document({"a": {"x": 1, "y": 2}})
        doc["a"].clear()
        assert doc.value == {"a": {}}
        reparsed = Document.parse(doc.as_toml())
        assert reparsed.value == doc.value

    def test_delete_all_keys_roundtrips(self) -> None:
        doc = Document({"a": {"x": 1, "y": 2}})
        del doc["a"]["x"]
        del doc["a"]["y"]
        assert doc.value == {"a": {}}
        reparsed = Document.parse(doc.as_toml())
        assert reparsed.value == doc.value

    def test_parsed_implicit_table(self) -> None:
        doc = Document.parse("[a.b]\nx = 1\n")
        assert doc["a"].implicit is True
        assert doc["a"]["b"].implicit is False
