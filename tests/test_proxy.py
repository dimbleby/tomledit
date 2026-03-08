"""Tests for Item proxy: len, iter, contains, bool, del, repr, dict/list methods."""

from __future__ import annotations

from datetime import date, datetime, time, timezone

import pytest

from tests.conftest import make_doc
from tomledit import Document

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

    def test_aot_len(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "c"\n'
        )
        assert len(doc["items"]) == 3


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

    def test_aot_iter(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "c"\n'
        )
        names = [entry["name"].value for entry in doc["items"]]
        assert names == ["a", "b", "c"]


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
    def test_empty_array_falsy(self) -> None:
        doc = Document.parse("arr = []\n")
        assert bool(doc["arr"]) is False

    def test_nonempty_array_truthy(self) -> None:
        doc = make_doc()
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
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert bool(doc["items"]) is True

    def test_datetime_truthy(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        assert bool(doc["dt"]) is True

    def test_date_truthy(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert bool(doc["d"]) is True

    def test_time_truthy(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        assert bool(doc["t"]) is True


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

    def test_del_aot_first(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        del doc["items"][0]
        assert len(doc["items"]) == 1
        assert doc["items"][0]["name"] == "b"

    def test_del_aot_negative(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        del doc["items"][-1]
        assert len(doc["items"]) == 1
        assert doc["items"][0]["name"] == "a"


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

    def test_aot_repr(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n')
        r = repr(doc["items"])
        assert "Item(" in r


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

    def test_clear_inline_table(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        doc["t"].clear()
        assert len(doc["t"]) == 0
        assert doc["t"] == {}


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

    def test_clear_aot(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        doc["items"].clear()
        assert len(doc["items"]) == 0

    def test_clear_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="clear"):
            doc["x"].clear()


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
# Negative indexing
# ---------------------------------------------------------------------------


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
        doc["arr"][1] = 20
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
        assert str(doc) == "arr = [1, 20, 30, 40, 5]\n"

    def test_delitem_slice_visible_in_output(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][0:2]
        assert str(doc) == "arr = [ 3, 4, 5]\n"

    # ---- additional edge cases ----

    def test_append_via_slice_at_end(self) -> None:
        """arr[len:len] = [...] should push new elements."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"][3:3] = [10, 20]
        assert doc["arr"] == [1, 2, 3, 10, 20]

    def test_replace_to_end_and_extend(self) -> None:
        """arr[2:] = [10, 20, 30] replaces from index 2 and adds extra."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"][2:] = [10, 20, 30]
        assert doc["arr"] == [1, 2, 10, 20, 30]

    def test_slice_assignment_on_table_raises(self) -> None:
        doc = Document.parse("[t]\nx = 1\n")
        with pytest.raises(TypeError, match="does not support slic"):
            doc["t"][0:1] = [1]

    def test_slice_delete_on_table_raises(self) -> None:
        doc = Document.parse("[t]\nx = 1\n")
        with pytest.raises(TypeError, match="does not support slic"):
            del doc["t"][0:1]

    def test_aot_slice_read(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "c"\n'
        )
        first_two = doc["items"][:2]
        assert len(first_two) == 2
        assert first_two[0]["name"] == "a"
        assert first_two[1]["name"] == "b"

    def test_aot_del_slice(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "c"\n'
        )
        del doc["items"][0:2]
        assert len(doc["items"]) == 1
        assert doc["items"][0]["name"] == "c"


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

    def test_aot_value(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\nvalue = 1\n[[items]]\nname = "b"\nvalue = 2\n'
        )
        v = doc["items"].value
        assert isinstance(v, list)
        assert len(v) == 2
        assert v[0] == {"name": "a", "value": 1}

    def test_datetime_with_offset(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00+05:30\n")
        v = doc["dt"].value
        assert isinstance(v, datetime)
        assert v.utcoffset().total_seconds() == 5 * 3600 + 30 * 60

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
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "c"\n'
        )
        assert doc["items"][0]["name"] == "a"
        assert doc["items"][2]["name"] == "c"

    def test_getitem_negative(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "c"\n'
        )
        assert doc["items"][-1]["name"] == "c"

    def test_getitem_out_of_range(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n')
        with pytest.raises(IndexError):
            doc["items"][99]

    def test_str(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert str(doc["items"]) == "[{'name': 'a'}]"


# ---------------------------------------------------------------------------
# Item: fmt
# ---------------------------------------------------------------------------


class TestProxyFmt:
    def test_fmt_table(self) -> None:
        doc = Document.parse("[t]\na   =   1\nb   =   2\n")
        doc["t"].fmt()
        assert str(doc) == "[t]\na = 1\nb = 2\n"

    def test_fmt_inline_table(self) -> None:
        doc = Document.parse("meta = {x = 1 }\n")
        doc["meta"]["y"] = 2
        doc["meta"].fmt()
        assert str(doc) == "meta = { x = 1, y = 2 }\n"

    def test_fmt_array(self) -> None:
        doc = Document.parse("arr = [  1,  2,  3  ]\n")
        doc["arr"].fmt()
        assert str(doc) == "arr = [1, 2, 3]\n"

    def test_fmt_array_of_tables_is_noop(self) -> None:
        text = "[[t]]\na   =   1\n[[t]]\nb   =   2\n"
        doc = Document.parse(text)
        doc["t"].fmt()
        assert str(doc) == text

    def test_fmt_scalar_is_noop(self) -> None:
        doc = Document.parse("x = 1\n")
        doc["x"].fmt()
        assert str(doc) == "x = 1\n"

    def test_fmt_does_not_invalidate_proxies(self) -> None:
        doc = Document.parse("[t]\na = 1\nb = 2\n")
        t = doc["t"]
        b = doc["t"]["b"]
        t.fmt()
        assert b.value == 2

    def test_fmt_table_strips_comments_on_entries(self) -> None:
        doc = Document.parse("# comment\na = 1 # inline\n")
        doc.fmt()
        assert str(doc) == "a = 1\n"
