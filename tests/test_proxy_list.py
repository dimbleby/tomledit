"""Tests for Item proxy: list-like methods and indexing."""

from __future__ import annotations

import pytest

from tomledit import Document

# ---------------------------------------------------------------------------
# Item: list-like methods (append, insert, pop, remove, extend, clear)
# ---------------------------------------------------------------------------


class TestProxyListMethods:
    def test_append(self, doc: Document) -> None:
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

    def test_append_on_table_raises(self, doc: Document) -> None:
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
# __iadd__ (+=)
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# __iadd__ (+=)
# ---------------------------------------------------------------------------


class TestIadd:
    def test_iadd_extends_array(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        doc["arr"] += [3, 4]
        assert doc["arr"] == [1, 2, 3, 4]

    def test_iadd_empty_iterable(self) -> None:
        doc = Document.parse("arr = [1]\n")
        doc["arr"] += []
        assert doc["arr"] == [1]

    def test_iadd_returns_same_proxy(self) -> None:
        doc = Document.parse("arr = [1]\n")
        proxy = doc["arr"]
        proxy += [2]
        assert proxy == [1, 2]
        assert doc["arr"] == [1, 2]

    def test_iadd_strings(self) -> None:
        doc = Document.parse('arr = ["a"]\n')
        doc["arr"] += ["b", "c"]
        assert doc["arr"] == ["a", "b", "c"]

    def test_iadd_on_table_raises(self, doc: Document) -> None:
        with pytest.raises(TypeError):
            doc["owner"] += [1]

    def test_iadd_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match=r"\+="):
            doc["x"] += [1]


# ---------------------------------------------------------------------------
# count() and index()
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# count() and index()
# ---------------------------------------------------------------------------


class TestCount:
    def test_count_integers(self) -> None:
        doc = Document.parse("arr = [1, 2, 2, 3, 2]\n")
        assert doc["arr"].count(2) == 3

    def test_count_zero_when_absent(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"].count(99) == 0

    def test_count_empty_array(self) -> None:
        doc = Document.parse("arr = []\n")
        assert doc["arr"].count(1) == 0

    def test_count_strings(self) -> None:
        doc = Document.parse('arr = ["a", "b", "a", "c"]\n')
        assert doc["arr"].count("a") == 2

    def test_count_mixed_types(self) -> None:
        doc = Document.parse('arr = [1, "a", true, 1]\n')
        assert doc["arr"].count(1) == 2
        assert doc["arr"].count("a") == 1
        assert doc["arr"].count(True) == 1

    def test_count_on_table_raises(self, doc: Document) -> None:
        with pytest.raises(TypeError):
            doc["owner"].count("name")

    def test_count_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError):
            doc["x"].count(42)

    def test_count_aot(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "a"\n'
        )
        assert doc["items"].count({"name": "a"}) == 2
        assert doc["items"].count({"name": "c"}) == 0

    def test_count_aot_non_dict_returns_zero(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert doc["items"].count("not a dict") == 0


class TestIndex:
    def test_index_found(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        assert doc["arr"].index(20) == 1

    def test_index_first_occurrence(self) -> None:
        doc = Document.parse("arr = [1, 2, 2, 3]\n")
        assert doc["arr"].index(2) == 1

    def test_index_missing_raises(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].index(99)

    def test_index_empty_raises(self) -> None:
        doc = Document.parse("arr = []\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].index(1)

    def test_index_with_start(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 2, 5]\n")
        assert doc["arr"].index(2, 2) == 3

    def test_index_with_start_and_stop(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 2, 5]\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].index(2, 2, 3)

    def test_index_negative_start(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 2, 5]\n")
        assert doc["arr"].index(2, -3) == 3

    def test_index_negative_stop(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 2, 5]\n")
        assert doc["arr"].index(2, 0, -3) == 1

    def test_index_strings(self) -> None:
        doc = Document.parse('arr = ["x", "y", "z"]\n')
        assert doc["arr"].index("z") == 2

    def test_index_on_table_raises(self, doc: Document) -> None:
        with pytest.raises(TypeError):
            doc["owner"].index("name")

    def test_index_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError):
            doc["x"].index(42)

    def test_index_aot(self) -> None:
        doc = Document.parse(
            '[[items]]\nname = "a"\n[[items]]\nname = "b"\n[[items]]\nname = "a"\n'
        )
        assert doc["items"].index({"name": "b"}) == 1
        assert doc["items"].index({"name": "a"}) == 0
        assert doc["items"].index({"name": "a"}, 1) == 2

    def test_index_aot_non_dict_raises(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n')
        with pytest.raises(ValueError, match="not in array"):
            doc["items"].index("not a dict")


# ---------------------------------------------------------------------------
# pop() returns native Python values
# ---------------------------------------------------------------------------


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
# Integer key on table-like items
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# Integer key on table-like items
# ---------------------------------------------------------------------------


class TestIntKeyOnTable:
    """Integer keys on tables should raise TypeError (TOML keys are strings)."""

    def test_getitem_int_on_table(self) -> None:
        doc = Document.parse("[t]\na = 1")
        with pytest.raises(TypeError, match="TOML table keys must be strings"):
            doc["t"][0]

    def test_getitem_int_on_inline_table(self) -> None:
        doc = Document.parse("t = {a = 1}")
        with pytest.raises(TypeError, match="TOML table keys must be strings"):
            doc["t"][0]

    def test_getitem_int_on_empty_table(self) -> None:
        doc = Document.parse("[t]")
        with pytest.raises(TypeError, match="TOML table keys must be strings"):
            doc["t"][0]


class TestStrKeyOnArray:
    """String keys on arrays should raise TypeError (array indices are integers)."""

    def test_getitem_str_on_array(self) -> None:
        doc = Document.parse("a = [1, 2, 3]")
        with pytest.raises(TypeError, match="TOML array indices must be integers"):
            doc["a"]["x"]

    def test_getitem_str_on_aot(self) -> None:
        doc = Document.parse("[[items]]\nname = 'a'\n")
        with pytest.raises(TypeError, match="TOML array indices must be integers"):
            doc["items"]["name"]

    def test_getitem_str_on_empty_array(self) -> None:
        doc = Document.parse("a = []")
        with pytest.raises(TypeError, match="TOML array indices must be integers"):
            doc["a"]["x"]


# ---------------------------------------------------------------------------
# get() with default
# ---------------------------------------------------------------------------


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
        proxies = doc["arr"][1:3]
        assert len(proxies) == 2
        doc["arr"][1] = 20
        # Re-fetch: the mutation is visible through the document.
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
