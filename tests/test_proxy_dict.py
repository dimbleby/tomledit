"""Tests for Item proxy: dict-like methods."""

from __future__ import annotations

from datetime import datetime

import pytest

from tomledit import Document

# ---------------------------------------------------------------------------
# Item: dict-like methods (keys, values, items, get, pop, update, etc.)
# ---------------------------------------------------------------------------


class TestProxyDictMethods:
    def test_keys(self, doc: Document) -> None:
        assert set(doc["owner"].keys()) == {"name", "age", "active"}

    def test_values(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        vals = doc["t"].values()
        assert len(vals) == 2

    def test_values_are_live_proxies(self) -> None:
        doc = Document.parse("[t]\n[t.inner]\nval = 10\n")
        vals = doc["t"].values()
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

    def test_get_existing(self, doc: Document) -> None:
        assert doc["owner"].get("name") == "Alice"

    def test_get_missing(self, doc: Document) -> None:
        assert doc["owner"].get("email") is None

    def test_get_returns_live_proxy(self) -> None:
        doc = Document.parse("[t]\n[t.inner]\nval = 10\n")
        inner = doc["t"].get("inner")
        assert inner is not None
        inner["val"] = 42
        assert doc["t"]["inner"]["val"] == 42

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

    def test_setdefault_missing(self, doc: Document) -> None:
        result = doc["owner"].setdefault("email", "default@example.com")
        assert result == "default@example.com"
        assert doc["owner"]["email"] == "default@example.com"

    def test_setdefault_existing(self, doc: Document) -> None:
        result = doc["owner"].setdefault("name", "fallback")
        assert result == "Alice"  # not overwritten

    def test_clear_table(self, doc: Document) -> None:
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

    def test_keys_on_scalar_raises(self, doc: Document) -> None:
        with pytest.raises(TypeError):
            doc["title"].keys()

    def test_clear_inline_table(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        doc["t"].clear()
        assert len(doc["t"]) == 0
        assert doc["t"] == {}


# ---------------------------------------------------------------------------
# Item: list-like methods (append, insert, pop, remove, extend, clear)
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# get() with default
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# pop() with default
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# pop() with default
# ---------------------------------------------------------------------------


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


# ---------------------------------------------------------------------------
# Slice indexing
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# pop() returns native Python values
# ---------------------------------------------------------------------------


class TestPopReturnsNative:
    """pop() should return native Python objects, not internal Item wrappers."""

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

    def test_pop_missing_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(KeyError):
            doc.pop("nope")

    def test_pop_missing_with_default(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc.pop("nope", 42) == 42

    def test_pop_removes_key(self, doc: Document) -> None:
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
# Negative indexing
# ---------------------------------------------------------------------------
