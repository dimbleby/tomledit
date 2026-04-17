"""Tests for Item proxy: list-like methods and indexing."""

from __future__ import annotations

import pytest

from tests.conftest import toml_literal
from tomledit import Document, ListItem

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

    def test_pop_by_index(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].pop(0)
        assert doc["arr"][0] == 2

    def test_pop_empty_raises(self) -> None:
        doc = Document.parse("arr = []\n")
        with pytest.raises(IndexError):
            doc["arr"].pop()

    def test_pop_with_explicit_none_default_raises_type_error(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(TypeError, match="positional arguments"):
            doc["arr"].pop(0, None)

    def test_pop_aot_last(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        val = doc["items"].pop()
        assert val == {"name": "b"}
        assert len(doc["items"]) == 1

    def test_pop_aot_by_index(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        val = doc["items"].pop(0)
        assert val == {"name": "a"}
        assert len(doc["items"]) == 1
        assert doc["items"][0] == {"name": "b"}

    def test_pop_aot_empty_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"].pop()
        with pytest.raises(IndexError):
            doc["items"].pop()

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

    def test_remove_empty_array_raises(self) -> None:
        doc = Document.parse("arr = []\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].remove(1)

    def test_remove_empty_aot_raises(self) -> None:
        doc = Document({"items": []})
        doc["items"].append({"a": 1})
        doc["items"].pop()
        with pytest.raises(ValueError, match="not in array"):
            doc["items"].remove({"a": 1})

    def test_remove_aot(self) -> None:
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
        doc["items"].remove({"name": "b"})
        assert len(doc["items"]) == 2
        assert doc["items"][0] == {"name": "a"}
        assert doc["items"][1] == {"name": "c"}

    def test_remove_aot_first_occurrence(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                [[items]]
                name = "b"
                [[items]]
                name = "a"
            """)
        )
        doc["items"].remove({"name": "a"})
        assert len(doc["items"]) == 2
        assert doc["items"][0] == {"name": "b"}
        assert doc["items"][1] == {"name": "a"}

    def test_remove_aot_missing_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        with pytest.raises(ValueError, match="not in array"):
            doc["items"].remove({"name": "z"})

    def test_remove_aot_non_dict_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        with pytest.raises(ValueError, match="not in array"):
            doc["items"].remove("not a dict")

    def test_remove_non_toml_object(self) -> None:
        """remove() raises ValueError for objects that aren't TOML-convertible."""

        class NotToml:
            pass

        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].remove(NotToml())

    # -- cross-type numeric equality in remove ----------------------------------

    def test_remove_float_finds_integer(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].remove(2.0)
        assert doc["arr"] == [1, 3]

    def test_remove_integer_finds_float(self) -> None:
        doc = Document.parse("arr = [1.0, 2.0, 3.0]\n")
        doc["arr"].remove(2)
        assert doc["arr"] == [1.0, 3.0]

    def test_remove_float_proxy_finds_integer(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\nx = 2.0\n")
        doc["arr"].remove(doc["x"])
        assert doc["arr"] == [1, 3]

    def test_remove_integer_proxy_finds_float(self) -> None:
        doc = Document.parse("arr = [1.0, 2.0, 3.0]\nx = 2\n")
        doc["arr"].remove(doc["x"])
        assert doc["arr"] == [1.0, 3.0]

    # -- boundary-decoration preservation on removal --------------------------

    def test_remove_first_preserves_padded_prefix(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3 ]\n")
        doc["arr"].remove(1)
        assert doc.as_toml() == "arr = [ 2, 3 ]\n"

    def test_remove_last_preserves_padded_suffix(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3 ]\n")
        doc["arr"].remove(3)
        assert doc.as_toml() == "arr = [ 1, 2 ]\n"

    def test_pop_last_preserves_padded_suffix(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3 ]\n")
        doc["arr"].pop()
        assert doc.as_toml() == "arr = [ 1, 2 ]\n"

    def test_del_first_preserves_prefix(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        del doc["arr"][0]
        assert doc.as_toml() == "arr = [2, 3]\n"

    def test_del_last_preserves_padded_suffix(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3 ]\n")
        del doc["arr"][-1]
        assert doc.as_toml() == "arr = [ 1, 2 ]\n"

    def test_extend(self) -> None:
        doc = Document.parse("arr = [1]\n")
        doc["arr"].extend([2, 3, 4])
        assert len(doc["arr"]) == 4
        assert doc["arr"][3] == 4

    def test_extend_self(self) -> None:
        """ListItem.extend(self) must work (self-referencing)."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        arr = doc["arr"]
        arr.extend(arr)
        assert arr == [1, 2, 3, 1, 2, 3]

    def test_append_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"].append({"name": "b"})
        assert len(doc["items"]) == 2
        assert doc["items"][1] == {"name": "b"}

    def test_append_aot_inline_table(self) -> None:
        src = Document.parse('x = {name = "b"}\n')
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"].append(src["x"])
        assert len(doc["items"]) == 2
        assert doc["items"][1] == {"name": "b"}

    def test_append_aot_non_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        with pytest.raises(TypeError, match="cannot append"):
            doc["items"].append(42)

    def test_insert_aot_beginning(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "b"
            [[items]]
            name = "c"
        """)
        )
        doc["items"].insert(0, {"name": "a"})
        assert len(doc["items"]) == 3
        assert doc["items"][0] == {"name": "a"}
        assert doc["items"][1] == {"name": "b"}
        assert doc["items"][2] == {"name": "c"}
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
            [[items]]
            name = "c"
        """)

    def test_insert_aot_beginning_spaced(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "b"

            [[items]]
            name = "c"
        """)
        )
        doc["items"].insert(0, {"name": "a"})
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"

            [[items]]
            name = "b"

            [[items]]
            name = "c"
        """)

    def test_insert_aot_middle(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "c"
        """)
        )
        doc["items"].insert(1, {"name": "b"})
        assert len(doc["items"]) == 3
        assert doc["items"][0] == {"name": "a"}
        assert doc["items"][1] == {"name": "b"}
        assert doc["items"][2] == {"name": "c"}
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
            [[items]]
            name = "c"
        """)

    def test_insert_aot_end(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"].insert(100, {"name": "c"})
        assert len(doc["items"]) == 3
        assert doc["items"][2] == {"name": "c"}
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
            [[items]]
            name = "c"
        """)

    def test_insert_aot_end_spaced(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"

            [[items]]
            name = "b"
        """)
        )
        doc["items"].insert(100, {"name": "c"})
        assert len(doc["items"]) == 3
        assert doc["items"][2] == {"name": "c"}
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"

            [[items]]
            name = "b"

            [[items]]
            name = "c"
        """)

    def test_extend_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"].extend([{"name": "b"}, {"name": "c"}])
        assert len(doc["items"]) == 3
        assert doc["items"][1] == {"name": "b"}
        assert doc["items"][2] == {"name": "c"}
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"

            [[items]]
            name = "b"

            [[items]]
            name = "c"
        """)

    def test_append_aot_entry_from_other_document(self) -> None:
        """Appending an AoT entry from another document renders in push order."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"

            [[items]]
            name = "b"
            """)
        )
        other = Document.parse('[[src]]\nname = "c"\n')
        doc["items"].append(other["src"][0])
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"

            [[items]]
            name = "b"

            [[items]]
            name = "c"
        """)

    def test_append_aot_compact(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"].append({"name": "c"})
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
            [[items]]
            name = "c"
        """)

    def test_append_aot_after_clear(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"].clear()
        doc["items"].append({"name": "b"})
        assert len(doc["items"]) == 1
        assert doc["items"][0] == {"name": "b"}
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "b"
        """)

    def test_insert_aot_beginning_single_element(self) -> None:
        """Inserting at 0 in a 1-element AoT should produce spaced output,
        matching the default spacing that append produces."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"].insert(0, {"name": "z"})
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "z"

            [[items]]
            name = "a"
        """)

    def test_clear_array(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].clear()
        assert len(doc["arr"]) == 0

    def test_append_on_table_raises(self, doc: Document) -> None:
        with pytest.raises(AttributeError):
            doc["owner"].append(1)

    def test_remove_string(self) -> None:
        doc = Document.parse('arr = ["a", "b", "c"]\n')
        doc["arr"].remove("b")
        assert len(doc["arr"]) == 2
        assert doc["arr"][0] == "a"
        assert doc["arr"][1] == "c"

    def test_clear_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        doc["items"].clear()
        assert len(doc["items"]) == 0

    def test_clear_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="clear"):
            doc["x"].clear()


# ---------------------------------------------------------------------------
# __iadd__ (+=)
# ---------------------------------------------------------------------------


class TestIadd:
    def test_iadd_extends_array(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        doc["arr"] += [3, 4]
        assert doc["arr"] == [1, 2, 3, 4]

    def test_iadd_self(self) -> None:
        """ListItem += self must work (self-referencing)."""
        doc = Document.parse("arr = [1, 2]\n")
        arr = doc["arr"]
        arr += arr
        assert arr == [1, 2, 1, 2]

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
        with pytest.raises(TypeError):
            doc["x"] += [1]

    def test_iadd_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        doc["items"] += [{"name": "b"}, {"name": "c"}]
        assert len(doc["items"]) == 3
        assert doc["items"][1] == {"name": "b"}
        assert doc["items"][2] == {"name": "c"}

    def test_iadd_proxy_preserves_inline_comments(self) -> None:
        """`a += b` must preserve b's formatting when b is a proxy, matching `a + b`."""
        doc = Document.parse(
            toml_literal("""
            a = [
                1,  # one
                2,  # two
            ]
            b = [
                3,  # three
                4,  # four
            ]
        """)
        )
        doc["a"] += doc["b"]
        assert doc["a"].as_toml() + "\n" == toml_literal("""
            [
                1,  # one
                2,  # two
                3,  # three
                4,  # four
            ]
        """)

    def test_iadd_dict_proxy_yields_keys(self) -> None:
        """`array += dict_proxy` yields table keys, matching `list += dict`."""
        doc = Document.parse("a = []\n[t]\nx = 1\ny = 2\n")
        doc["a"] += doc["t"]
        assert list(doc["a"]) == ["x", "y"]

    def test_extend_aot_proxy_converts_to_inline_tables(self) -> None:
        """`array.extend(aot_proxy)` falls back to per-element conversion."""
        doc = Document.parse("a = []\n[[t]]\nx = 1\n[[t]]\nx = 2\n")
        doc["a"].extend(doc["t"])
        assert doc["a"] == [{"x": 1}, {"x": 2}]

    def test_extend_array_proxy_into_aot_raises(self) -> None:
        """`aot.extend(array_proxy)` raises TypeError — scalars can't be AoT tables."""
        doc = Document.parse("b = [1, 2, 3]\n[[a]]\nx = 1\n")
        with pytest.raises(TypeError):
            doc["a"].extend(doc["b"])

    def test_extend_aot_from_other_document_preserves_order(self) -> None:
        """Extending an AoT with an AoT from another document appends in order.

        Without clearing source positions, toml_edit renders AoT entries in
        span-position order, which interleaves entries cloned from another
        document.
        """
        text = toml_literal("""
            [[a]]
            x = 1

            [[a]]
            x = 2
        """)
        doc = Document.parse(text)
        doc2 = Document.parse(text)
        doc["a"].extend(doc2["a"])
        assert [dict(t) for t in doc["a"]] == [
            {"x": 1},
            {"x": 2},
            {"x": 1},
            {"x": 2},
        ]
        assert doc.as_toml() == toml_literal("""
            [[a]]
            x = 1

            [[a]]
            x = 2

            [[a]]
            x = 1

            [[a]]
            x = 2
        """)

    def test_iadd_aot_from_other_document_preserves_order(self) -> None:
        """`aot += other_aot` from a different document appends in order."""
        text = toml_literal("""
            [[a]]
            x = 1

            [[a]]
            x = 2
        """)
        doc = Document.parse(text)
        doc2 = Document.parse(text)
        doc["a"] += doc2["a"]
        assert [dict(t) for t in doc["a"]] == [
            {"x": 1},
            {"x": 2},
            {"x": 1},
            {"x": 2},
        ]
        assert doc.as_toml() == toml_literal("""
            [[a]]
            x = 1

            [[a]]
            x = 2

            [[a]]
            x = 1

            [[a]]
            x = 2
        """)


class TestAdd:
    """list + list returns a new ListItem (non-mutating, format-preserving)."""

    def test_add_two_arrays(self) -> None:
        doc = Document({"a": [1, 2], "b": [3, 4]})
        result = doc["a"] + doc["b"]
        assert result == [1, 2, 3, 4]
        assert isinstance(result, ListItem)

    def test_add_array_and_plain_list(self) -> None:
        doc = Document({"a": [1, 2]})
        result = doc["a"] + [3, 4]
        assert result == [1, 2, 3, 4]
        assert isinstance(result, ListItem)

    def test_add_does_not_mutate_document(self) -> None:
        doc = Document({"a": [1, 2]})
        _ = doc["a"] + [3]
        assert doc["a"] == [1, 2]

    def test_radd_plain_list_plus_array(self) -> None:
        doc = Document({"a": [3, 4]})
        result = [1, 2] + doc["a"]
        assert result == [1, 2, 3, 4]
        assert isinstance(result, ListItem)

    def test_add_empty(self) -> None:
        doc = Document({"a": [1, 2]})
        assert doc["a"] + [] == [1, 2]
        assert [] + doc["a"] == [1, 2]

    def test_add_non_iterable_returns_not_implemented(self) -> None:
        doc = Document({"a": [1, 2]})
        with pytest.raises(TypeError):
            _ = doc["a"] + 42

    def test_radd_non_iterable_returns_not_implemented(self) -> None:
        doc = Document({"a": [1, 2]})
        with pytest.raises(TypeError):
            _ = 42 + doc["a"]

    def test_add_preserves_formatting(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
                # first
                1,
                # second
                2,
            ]
        """)
        )
        result = doc["arr"] + [3]
        assert result.as_toml() + "\n" == toml_literal("""
            [
                # first
                1,
                # second
                2,
                3,
            ]
        """)

    def test_add_two_listitem_preserves_comments(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = [
                # x
                1,
            ]
            b = [
                # y
                2,
            ]
        """)
        )
        result = doc["a"] + doc["b"]
        assert result.as_toml() + "\n" == toml_literal("""
            [
                # x
                1,
                # y
                2,
            ]
        """)

    def test_radd_two_listitems(self) -> None:
        doc = Document({"a": [1, 2], "b": [3, 4]})
        result = doc["b"] + doc["a"]
        assert result == [3, 4, 1, 2]
        assert isinstance(result, ListItem)

    def test_add_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        result = doc["items"] + [{"name": "b"}]
        assert len(result) == 2
        assert result[1] == {"name": "b"}

    def test_radd_aot(self) -> None:
        """__radd__ on an AoT exercises empty_array_like for AoT kind."""
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        result = [{"name": "z"}] + doc["items"]
        assert len(result) == 2
        assert result[0] == {"name": "z"}
        assert result[1] == {"name": "a"}

    def test_add_array_plus_aot(self) -> None:
        """Cross-kind: plain array + AoT works (falls back to value extraction)."""
        doc = Document.parse(
            toml_literal("""
            arr = [1, 2]

            [[tbl]]
            name = "a"
        """)
        )
        result = doc["arr"] + doc["tbl"]
        assert result.as_toml() == '[1, 2, { name = "a" }]'
        assert isinstance(result, ListItem)

    def test_add_aot_plus_array(self) -> None:
        """Cross-kind: AoT + plain array raises TypeError (AoT only holds tables)."""
        doc = Document.parse(
            toml_literal("""
            arr = [1, 2]

            [[tbl]]
            name = "a"
        """)
        )
        with pytest.raises(TypeError):
            _ = doc["tbl"] + doc["arr"]

    def test_mul_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        result = doc["items"] * 2
        assert len(result) == 2
        assert result[0] == {"name": "a"}
        assert result[1] == {"name": "a"}


class TestMul:
    """list * n returns a new repeated ListItem (non-mutating, format-preserving)."""

    def test_mul_repeat(self) -> None:
        doc = Document({"a": [1, 2]})
        result = doc["a"] * 3
        assert result == [1, 2, 1, 2, 1, 2]
        assert isinstance(result, ListItem)

    def test_rmul(self) -> None:
        doc = Document({"a": [1, 2]})
        result = 3 * doc["a"]
        assert result == [1, 2, 1, 2, 1, 2]
        assert isinstance(result, ListItem)

    def test_mul_zero(self) -> None:
        doc = Document({"a": [1, 2]})
        assert doc["a"] * 0 == []

    def test_mul_one(self) -> None:
        doc = Document({"a": [1, 2]})
        assert doc["a"] * 1 == [1, 2]

    def test_mul_does_not_mutate_document(self) -> None:
        doc = Document({"a": [1, 2]})
        _ = doc["a"] * 3
        assert doc["a"] == [1, 2]

    def test_imul(self) -> None:
        doc = Document({"a": [1, 2]})
        doc["a"] *= 3
        assert doc["a"] == [1, 2, 1, 2, 1, 2]

    def test_imul_zero(self) -> None:
        doc = Document({"a": [1, 2]})
        doc["a"] *= 0
        assert doc["a"] == []

    def test_imul_one_is_noop(self) -> None:
        doc = Document({"a": [1, 2]})
        doc["a"] *= 1
        assert doc["a"] == [1, 2]

    def test_imul_aot_preserves_blank_line_spacing(self) -> None:
        """Repeating a blank-line-style AoT keeps blank lines at the seam."""
        doc = Document.parse(
            toml_literal("""
            [[a]]
            x = 1

            [[a]]
            x = 2
            """)
        )
        doc["a"] *= 2
        assert doc.as_toml() == toml_literal("""
            [[a]]
            x = 1

            [[a]]
            x = 2

            [[a]]
            x = 1

            [[a]]
            x = 2
        """)

    def test_imul_aot_preserves_compact_style(self) -> None:
        """Repeating a compact-style AoT stays compact."""
        doc = Document.parse(
            toml_literal("""
            [[a]]
            x = 1
            [[a]]
            x = 2
            """)
        )
        doc["a"] *= 2
        assert doc.as_toml() == toml_literal("""
            [[a]]
            x = 1
            [[a]]
            x = 2
            [[a]]
            x = 1
            [[a]]
            x = 2
        """)

    def test_imul_preserves_formatting(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
                # first
                1,
                # second
                2,
            ]
        """)
        )
        doc["arr"] *= 2
        assert doc.as_toml() == toml_literal("""
            arr = [
                # first
                1,
                # second
                2,
                # first
                1,
                # second
                2,
            ]
        """)

    def test_mul_preserves_formatting(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
                # first
                1,
                # second
                2,
            ]
        """)
        )
        result = doc["arr"] * 2
        assert result.as_toml() + "\n" == toml_literal("""
            [
                # first
                1,
                # second
                2,
                # first
                1,
                # second
                2,
            ]
        """)

    def test_mul_preserves_inline_comments(self) -> None:
        """Inline comments on every element survive __mul__ at the seam."""
        doc = Document.parse(
            toml_literal("""
            arr = [
                1,  # one
                2,  # two
            ]
        """)
        )
        result = doc["arr"] * 2
        assert result.as_toml() + "\n" == toml_literal("""
            [
                1,  # one
                2,  # two
                1,  # one
                2,  # two
            ]
        """)

    def test_add_preserves_inline_comments(self) -> None:
        """Inline comments on every element survive __add__ at the seam."""
        doc = Document.parse(
            toml_literal("""
            a = [
                1,  # one
                2,  # two
            ]
            b = [
                3,  # three
                4,  # four
            ]
        """)
        )
        result = doc["a"] + doc["b"]
        assert result.as_toml() + "\n" == toml_literal("""
            [
                1,  # one
                2,  # two
                3,  # three
                4,  # four
            ]
        """)

    def test_mul_single_line_spacing(self) -> None:
        """Repeated single-line arrays keep the ', ' separator at every seam."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert (doc["arr"] * 2).as_toml() == "[1, 2, 3, 1, 2, 3]"
        assert (3 * doc["arr"]).as_toml() == "[1, 2, 3, 1, 2, 3, 1, 2, 3]"

    def test_mul_no_space_style_preserved(self) -> None:
        """A no-space source stays no-space when repeated."""
        doc = Document.parse("arr = [1,2,3]\n")
        assert (doc["arr"] * 2).as_toml() == "[1,2,3,1,2,3]"
        assert (3 * doc["arr"]).as_toml() == "[1,2,3,1,2,3,1,2,3]"

    def test_add_single_line_spacing(self) -> None:
        """Concatenating single-line arrays keeps the ', ' separator at the seam."""
        doc = Document({"a": [1, 2], "b": [3, 4]})
        assert (doc["a"] + doc["b"]).as_toml() == "[1, 2, 3, 4]"
        assert (doc["a"] + [5, 6]).as_toml() == "[1, 2, 5, 6]"
        assert ([0] + doc["a"]).as_toml() == "[0, 1, 2]"

    def test_add_no_space_style_preserved(self) -> None:
        """Concatenation onto a no-space array stays no-space."""
        doc = Document.parse("a = [1,2]\nb = [3,4]\n")
        assert (doc["a"] + doc["b"]).as_toml() == "[1,2,3,4]"
        assert (doc["a"] + [5, 6]).as_toml() == "[1,2,5,6]"

    def test_append_no_space_style_preserved(self) -> None:
        """append() on a no-space array stays no-space."""
        doc = Document.parse("arr = [1,2,3]\n")
        doc["arr"].append(4)
        assert doc.as_toml() == "arr = [1,2,3,4]\n"

    def test_extend_no_space_style_preserved(self) -> None:
        """extend() on a no-space array stays no-space."""
        doc = Document.parse("arr = [1,2,3]\n")
        doc["arr"].extend([4, 5])
        assert doc.as_toml() == "arr = [1,2,3,4,5]\n"

    def test_insert_no_space_style_preserved(self) -> None:
        """insert() at the end of a no-space array stays no-space."""
        doc = Document.parse("arr = [1,2,3]\n")
        doc["arr"].insert(3, 4)
        assert doc.as_toml() == "arr = [1,2,3,4]\n"


# ---------------------------------------------------------------------------
# count() and index()
# ---------------------------------------------------------------------------


class TestCount:
    def test_count_integers(self) -> None:
        doc = Document.parse("arr = [1, 2, 2, 3, 2]\n")
        assert doc["arr"].count(2) == 3

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
        with pytest.raises(AttributeError):
            doc["owner"].count("name")

    def test_count_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(AttributeError):
            doc["x"].count(42)

    def test_count_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                [[items]]
                name = "b"
                [[items]]
                name = "a"
            """)
        )
        assert doc["items"].count({"name": "a"}) == 2
        assert doc["items"].count({"name": "c"}) == 0

    def test_count_aot_non_dict_returns_zero(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        assert doc["items"].count("not a dict") == 0


class TestIndex:
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

    def test_index_negative_start(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 2, 5]\n")
        assert doc["arr"].index(2, -3) == 3

    def test_index_strings(self) -> None:
        doc = Document.parse('arr = ["x", "y", "z"]\n')
        assert doc["arr"].index("z") == 2

    def test_index_on_table_raises(self, doc: Document) -> None:
        with pytest.raises(AttributeError):
            doc["owner"].index("name")

    def test_index_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(AttributeError):
            doc["x"].index(42)

    def test_index_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                [[items]]
                name = "b"
                [[items]]
                name = "a"
            """)
        )
        assert doc["items"].index({"name": "b"}) == 1
        assert doc["items"].index({"name": "a"}) == 0
        assert doc["items"].index({"name": "a"}, 1) == 2

    def test_index_aot_non_dict_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        with pytest.raises(ValueError, match="not in array"):
            doc["items"].index("not a dict")

    def test_index_non_toml_object(self) -> None:
        """index() raises ValueError for objects that aren't TOML-convertible."""

        class NotToml:
            pass

        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].index(NotToml())


# ---------------------------------------------------------------------------
# Proxy arguments (ItemProxy passed to remove / __contains__ / count / index)
# ---------------------------------------------------------------------------


class TestProxyArguments:
    """Operations that compare values should accept ItemProxy (ScalarItem) args."""

    def test_remove_scalar_proxy_int(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        proxy = doc["arr"][1]
        doc["arr"].remove(proxy)
        assert doc["arr"] == [1, 3]

    def test_remove_scalar_proxy_string(self) -> None:
        doc = Document.parse('arr = ["a", "b", "c"]\n')
        proxy = doc["arr"][1]
        doc["arr"].remove(proxy)
        assert doc["arr"] == ["a", "c"]

    def test_remove_scalar_proxy_bool(self) -> None:
        doc = Document.parse("arr = [true, false, true]\n")
        proxy = doc["arr"][1]
        doc["arr"].remove(proxy)
        assert doc["arr"] == [True, True]

    def test_remove_proxy_preserves_formatting(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3 ]\n")
        proxy = doc["arr"][1]
        doc["arr"].remove(proxy)
        assert doc.as_toml() == "arr = [ 1, 3 ]\n"

    def test_contains_scalar_proxy_string(self) -> None:
        doc = Document.parse('arr = ["x", "y", "z"]\n')
        proxy = doc["arr"][1]
        assert proxy in doc["arr"]

    def test_contains_scalar_proxy_int(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        proxy = doc["arr"][2]
        assert proxy in doc["arr"]

    def test_contains_scalar_proxy_missing(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        other = Document.parse("x = 99\n")
        assert other["x"] not in doc["arr"]

    def test_count_scalar_proxy(self) -> None:
        doc = Document.parse('arr = ["a", "b", "a", "c"]\n')
        proxy = doc["arr"][0]
        assert doc["arr"].count(proxy) == 2

    def test_index_scalar_proxy(self) -> None:
        doc = Document.parse('arr = ["x", "y", "z"]\n')
        proxy = doc["arr"][2]
        assert doc["arr"].index(proxy) == 2

    def test_index_scalar_proxy_with_start(self) -> None:
        doc = Document.parse("arr = [1, 2, 3, 2, 5]\n")
        proxy = doc["arr"][1]  # value 2
        assert doc["arr"].index(proxy, 2) == 3

    def test_remove_cross_document_proxy(self) -> None:
        doc1 = Document.parse("arr = [1, 2, 3]\n")
        doc2 = Document.parse("x = 2\n")
        doc1["arr"].remove(doc2["x"])
        assert doc1["arr"] == [1, 3]


# ---------------------------------------------------------------------------
# Negative indexing
# ---------------------------------------------------------------------------


class TestNegativeIndexing:
    """Negative indices should work like Python lists."""

    def test_proxy_getitem_minus_one(self) -> None:
        doc = Document.parse("arr = [10, 20, 30]\n")
        assert doc["arr"][-1] == 30

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


class TestIntKeyOnTable:
    """Integer keys on tables should raise TypeError (TOML keys are strings)."""

    def test_getitem_int_on_inline_table(self) -> None:
        doc = Document.parse("t = {a = 1}")
        with pytest.raises(TypeError, match="keys must be strings"):
            doc["t"][0]

    def test_getitem_int_on_empty_table(self) -> None:
        doc = Document.parse("[t]")
        with pytest.raises(TypeError, match="keys must be strings"):
            doc["t"][0]


class TestStrKeyOnArray:
    """String keys on arrays should raise TypeError (array indices are integers)."""

    def test_getitem_str_on_array(self) -> None:
        doc = Document.parse("a = [1, 2, 3]")
        with pytest.raises(TypeError, match="TOML array indices must be integers"):
            doc["a"]["x"]

    def test_getitem_str_on_aot(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = 'a'
        """)
        )
        with pytest.raises(TypeError, match="TOML array indices must be integers"):
            doc["items"]["name"]


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

    def test_negative_start(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][-2:]] == [4, 5]

    def test_negative_step_reverse(self) -> None:
        doc = Document.parse(self.TOML)
        assert [int(str(x)) for x in doc["arr"][::-1]] == [5, 4, 3, 2, 1]

    def test_empty_slice(self) -> None:
        doc = Document.parse(self.TOML)
        assert doc["arr"][2:2] == []

    def test_slice_returns_proxies(self) -> None:
        """Each element of the returned list is still a live proxy."""
        doc = Document.parse(self.TOML)
        proxies = doc["arr"][1:3]
        assert len(proxies) == 2
        doc["arr"][1] = 20
        # Re-fetch: the mutation is visible through the document.
        assert doc["arr"][1] == 20

    # ---- __setitem__ slices ----

    def test_setitem_grow(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][1:3] = [20, 30, 40]
        assert doc["arr"] == [1, 20, 30, 40, 4, 5]

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

    def test_delitem_negative_step(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][::-2]
        assert doc["arr"] == [2, 4]

    def test_delitem_negative_step_range(self) -> None:
        doc = Document.parse(self.TOML)
        del doc["arr"][3:0:-1]
        assert doc["arr"] == [1, 5]

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
        doc = Document.parse(
            toml_literal("""
            [t]
            x = 1
        """)
        )
        with pytest.raises(TypeError, match="keys must be strings"):
            doc["t"][1:3]

    def test_slice_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 'hello'\n")
        with pytest.raises(TypeError):
            doc["x"][1:3]

    # ---- mutation visible in document ----

    def test_setitem_slice_visible_in_output(self) -> None:
        doc = Document.parse(self.TOML)
        doc["arr"][1:4] = [20, 30, 40]
        assert doc.as_toml() == "arr = [1, 20, 30, 40, 5]\n"

    def test_delitem_slice_preserves_padded_first(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3, 4, 5 ]\n")
        del doc["arr"][0:2]
        assert doc.as_toml() == "arr = [ 3, 4, 5 ]\n"

    def test_delitem_slice_preserves_padded_last(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3, 4, 5 ]\n")
        del doc["arr"][3:5]
        assert doc.as_toml() == "arr = [ 1, 2, 3 ]\n"

    def test_setitem_slice_empty_preserves_padded_first(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3, 4, 5 ]\n")
        doc["arr"][0:2] = []
        assert doc.as_toml() == "arr = [ 3, 4, 5 ]\n"

    def test_setitem_slice_empty_preserves_padded_last(self) -> None:
        doc = Document.parse("arr = [ 1, 2, 3, 4, 5 ]\n")
        doc["arr"][3:5] = []
        assert doc.as_toml() == "arr = [ 1, 2, 3 ]\n"

    def test_aot_setitem_slice_empty_removes_leading_blank_line(self) -> None:
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
        doc["items"][0:1] = []
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "b"

            [[items]]
            name = "c"
        """)

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

    def test_slice_assign_self(self) -> None:
        """arr[:] = arr must work (self-referencing)."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        arr = doc["arr"]
        arr[:] = arr
        assert arr == [1, 2, 3]

    def test_slice_assignment_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            x = 1
        """)
        )
        with pytest.raises(TypeError, match="keys must be strings"):
            doc["t"][0:1] = [1]

    def test_slice_delete_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            x = 1
        """)
        )
        with pytest.raises(TypeError, match="keys must be strings"):
            del doc["t"][0:1]

    def test_aot_slice_read(self) -> None:
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
        first_two = doc["items"][:2]
        assert len(first_two) == 2
        assert first_two[0]["name"] == "a"
        assert first_two[1]["name"] == "b"

    def test_aot_del_slice(self) -> None:
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
        del doc["items"][0:2]
        assert len(doc["items"]) == 1
        assert doc["items"][0]["name"] == "c"

    def test_aot_set_slice_contiguous(self) -> None:
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
        doc["items"][0:2] = [{"name": "x"}]
        assert len(doc["items"]) == 2
        assert doc["items"][0]["name"] == "x"
        assert doc["items"][1]["name"] == "c"

    def test_aot_set_slice_extended(self) -> None:
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
        doc["items"][0:3:2] = [{"name": "x"}, {"name": "z"}]
        assert len(doc["items"]) == 3
        assert doc["items"][0]["name"] == "x"
        assert doc["items"][1]["name"] == "b"
        assert doc["items"][2]["name"] == "z"

    def test_aot_set_slice_extended_size_mismatch_raises(self) -> None:
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
        with pytest.raises(ValueError, match="attempt to assign sequence of size"):
            doc["items"][0:3:2] = [{"name": "x"}]

    def test_aot_del_slice_negative_step(self) -> None:
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
        del doc["items"][2::-1]
        assert len(doc["items"]) == 0


# ---------------------------------------------------------------------------
# Multiline array formatting preservation
# ---------------------------------------------------------------------------

MULTILINE_ARRAY = """\
arr = [
    1,
    2,
    3,
]
"""


class TestBoundarySpacePreservation:
    """Inserting at boundaries should preserve the space after `[` and before `]`."""

    def test_append_transfers_trailing_space(self) -> None:
        doc = Document.parse("arr = [1, 2 ]\n")
        doc["arr"].append(3)
        assert doc.as_toml() == "arr = [1, 2, 3 ]\n"

    def test_extend_transfers_trailing_space(self) -> None:
        doc = Document.parse("arr = [1, 2 ]\n")
        doc["arr"].extend([3, 4])
        assert doc.as_toml() == "arr = [1, 2, 3, 4 ]\n"

    def test_insert_at_end_transfers_trailing_space(self) -> None:
        doc = Document.parse("arr = [1, 2 ]\n")
        doc["arr"].insert(100, 3)
        assert doc.as_toml() == "arr = [1, 2, 3 ]\n"

    def test_insert_at_start_preserves_leading_space(self) -> None:
        doc = Document.parse("arr = [ 1, 2]\n")
        doc["arr"].insert(0, 0)
        assert doc.as_toml() == "arr = [ 0, 1, 2]\n"

    def test_insert_in_middle_keeps_trailing_space(self) -> None:
        doc = Document.parse("arr = [1, 2 ]\n")
        doc["arr"].insert(1, 3)
        assert doc.as_toml() == "arr = [1, 3, 2 ]\n"

    def test_slice_replace_first_preserves_leading_space(self) -> None:
        doc = Document.parse("arr = [ 1, 2 ]\n")
        doc["arr"][0:1] = [9]
        assert doc.as_toml() == "arr = [ 9, 2 ]\n"

    def test_slice_replace_last_preserves_trailing_space(self) -> None:
        doc = Document.parse("arr = [ 1, 2 ]\n")
        doc["arr"][1:2] = [9]
        assert doc.as_toml() == "arr = [ 1, 9 ]\n"

    def test_slice_replace_all_preserves_both_spaces(self) -> None:
        doc = Document.parse("arr = [ 1, 2 ]\n")
        doc["arr"][0:2] = [9, 8]
        assert doc.as_toml() == "arr = [ 9, 8 ]\n"

    def test_slice_insert_at_start_preserves_leading_space(self) -> None:
        doc = Document.parse("arr = [ 1, 2 ]\n")
        doc["arr"][0:0] = [9]
        assert doc.as_toml() == "arr = [ 9, 1, 2 ]\n"

    def test_slice_insert_at_end_preserves_trailing_space(self) -> None:
        doc = Document.parse("arr = [ 1, 2 ]\n")
        doc["arr"][2:2] = [9]
        assert doc.as_toml() == "arr = [ 1, 2, 9 ]\n"


class TestMultilineFormatPreservation:
    def test_insert_at_start_preserves_multiline(self) -> None:
        doc = Document.parse(MULTILINE_ARRAY)
        doc["arr"].insert(0, 0)
        assert doc.as_toml() == toml_literal("""
            arr = [
                0,
                1,
                2,
                3,
            ]
        """)

    def test_extend_preserves_multiline(self) -> None:
        doc = Document.parse(MULTILINE_ARRAY)
        doc["arr"].extend([4, 5])
        assert doc.as_toml() == toml_literal("""
            arr = [
                1,
                2,
                3,
                4,
                5,
            ]
        """)

    def test_iadd_preserves_multiline(self) -> None:
        doc = Document.parse(MULTILINE_ARRAY)
        doc["arr"] += [4, 5]
        assert doc.as_toml() == toml_literal("""
            arr = [
                1,
                2,
                3,
                4,
                5,
            ]
        """)

    def test_append_preserves_comments(self) -> None:
        doc = Document.parse(
            toml_literal("""
            arr = [
                1, # first
                2, # second
                3,
            ]
        """)
        )
        doc["arr"].append(4)
        assert doc.as_toml() == toml_literal("""
            arr = [
                1, # first
                2, # second
                3,
                4,
            ]
        """)


# ---------------------------------------------------------------------------
# set_multiline
# ---------------------------------------------------------------------------


class TestSetMultiline:
    def test_basic(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].set_multiline()
        assert doc.as_toml() == toml_literal("""
            arr = [
                1,
                2,
                3,
            ]
        """)

    def test_empty_array_is_noop(self) -> None:
        doc = Document.parse("arr = []\n")
        doc["arr"].set_multiline()
        assert doc.as_toml() == "arr = []\n"

    def test_fmt_collapses_back(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        doc["arr"].set_multiline()
        doc["arr"].fmt()
        assert doc.as_toml() == "arr = [1, 2, 3]\n"

    def test_new_array_then_multiline(self) -> None:
        doc = Document.parse("")
        doc["deps"] = ["a", "b", "c"]
        doc["deps"].set_multiline()
        assert doc.as_toml() == toml_literal("""
            deps = [
                "a",
                "b",
                "c",
            ]
        """)

    def test_on_non_array_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(AttributeError):
            doc["x"].set_multiline()

    def test_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(AttributeError):
            doc["t"].set_multiline()

    def test_on_aot_is_noop(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        before = doc.as_toml()
        doc["items"].set_multiline()
        assert doc.as_toml() == before

    def test_nested_array(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [pkg]
            deps = [1, 2]
        """)
        )
        doc["pkg"]["deps"].set_multiline()
        assert doc.as_toml() == toml_literal("""
            [pkg]
            deps = [
                1,
                2,
            ]
        """)

    def test_append_inherits_after_set_multiline(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        doc["arr"].set_multiline()
        doc["arr"].append(3)
        assert doc.as_toml() == toml_literal("""
            arr = [
                1,
                2,
                3,
            ]
        """)


# ---------------------------------------------------------------------------
# Bug regression tests
# ---------------------------------------------------------------------------


class TestEmptySliceDeletion:
    """del arr[:] on an empty array must not panic from usize underflow."""

    def test_del_all_slice_empty_array(self) -> None:
        doc = Document.parse("arr = []\n")
        del doc["arr"][:]
        assert list(doc["arr"]) == []

    def test_del_specific_slice_empty_array(self) -> None:
        doc = Document.parse("arr = []\n")
        del doc["arr"][0:0]
        assert list(doc["arr"]) == []

    def test_del_step_slice_empty_array(self) -> None:
        doc = Document.parse("arr = []\n")
        del doc["arr"][::2]
        assert list(doc["arr"]) == []


# ---------------------------------------------------------------------------
# Write-lock deadlock regression tests
#
# These verify that list operations do not deadlock when Python callbacks
# (__eq__, __index__) access the same document.
# Each test should complete instantly; a hang means a deadlock.
# ---------------------------------------------------------------------------


class TestWriteLockDeadlocks:
    """Python callbacks (__eq__, __index__) that read the document must not
    deadlock when called from list operations that hold a write lock.
    """

    def test_pop_custom_index_reads_document(self) -> None:
        """pop() must not deadlock when __index__ reads the same document."""

        class Tricky:
            def __init__(self, doc: Document) -> None:
                self.doc = doc

            def __index__(self) -> int:
                return len(self.doc["arr"]) - 1

        doc = Document.parse("arr = [1, 2, 3]\n")
        tricky = Tricky(doc)
        val = doc["arr"].pop(tricky)
        assert val == 3
        assert doc["arr"] == [1, 2]
