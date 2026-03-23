"""Tests for error handling: no panics, only proper Python exceptions."""

from __future__ import annotations

import pytest

from tests.conftest import toml_literal
from tomledit import Document

# ---------------------------------------------------------------------------
# Parse errors
# ---------------------------------------------------------------------------


class TestParseErrors:
    def test_duplicate_key_raises(self) -> None:
        with pytest.raises(ValueError, match="duplicate"):
            Document.parse(
                toml_literal("""
                x = 1
                x = 2
            """)
            )


# ---------------------------------------------------------------------------
# Unsupported Python types assigned to TOML keys
# ---------------------------------------------------------------------------


class TestUnsupportedValueTypes:
    """Python types without obvious TOML semantics should be rejected."""

    def test_none_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="None is not a valid TOML value"):
            doc["x"] = None

    def test_bytearray_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError):
            doc["x"] = bytearray(b"hello")


# ---------------------------------------------------------------------------
# Operations on wrong item types
# ---------------------------------------------------------------------------


class TestWrongTypeErrors:
    """Each method should raise TypeError when called on the wrong item type."""

    # -- subscript on scalars --

    def test_setitem_on_bool_raises(self) -> None:
        doc = Document.parse("flag = true\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["flag"][0] = False

    def test_setitem_on_float_raises(self) -> None:
        doc = Document.parse("val = 1.5\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["val"][0] = 2

    def test_getitem_int_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["x"][0]

    def test_getitem_str_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["x"]["y"]

    def test_delitem_on_scalar_raises(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            del doc["val"][0]

    def test_contains_on_scalar_raises(self) -> None:
        doc = Document.parse("val = 42\n")
        with pytest.raises(TypeError):
            assert 1 in doc["val"]

    def test_slice_del_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="does not support slice deletion"):
            del doc["x"][0:1]

    def test_slice_assign_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="does not support slice assignment"):
            doc["x"][0:1] = [1]

    # -- wrong key type --

    def test_getitem_float_key_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [tbl]
            x = 1
        """)
        )
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            doc["tbl"][1.5]  # type: ignore[call-overload]

    def test_setitem_int_key_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(TypeError, match="strings, not integers"):
            doc["t"][0] = 99

    def test_setitem_str_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="integers, not strings"):
            doc["arr"]["x"] = 99

    def test_delitem_int_key_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(TypeError, match="strings, not integers"):
            del doc["t"][0]

    def test_delitem_str_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="integers, not strings"):
            del doc["arr"]["x"]

    def test_setitem_float_key_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            doc["t"][1.5] = 99  # type: ignore[index]

    def test_delitem_float_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            del doc["arr"][1.5]  # type: ignore[arg-type]

    # -- missing keys --

    def test_getitem_missing_nested_key_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(KeyError, match="nonexistent"):
            doc["t"]["nonexistent"]

    def test_delitem_inline_table_missing_key_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(KeyError):
            del doc["t"]["nonexistent"]

    # -- dict methods on wrong types --

    def test_get_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(AttributeError):
            doc["arr"].get("x")

    def test_get_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(AttributeError):
            doc["x"].get("y")

    def test_pop_noarg_on_inline_table_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(TypeError, match="missing 1 required positional argument"):
            doc["t"].pop()

    # -- list methods on wrong types --

    def test_extend_on_table_raises(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        with pytest.raises(AttributeError):
            doc["t"].extend([1, 2])

    # -- view methods on wrong types --


# ---------------------------------------------------------------------------
# Atomicity: a bad element must not leave the collection partially mutated
# ---------------------------------------------------------------------------


class TestAtomicMutation:
    """Conversion errors must be raised *before* any mutation happens.

    Each test supplies a mix of valid and invalid values so that without
    up-front validation the collection would be left in a half-modified state.
    """

    # -- array-of-tables --

    def test_aot_setitem_slice_contiguous_rolls_back(self) -> None:
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
        with pytest.raises(TypeError, match="cannot append"):
            doc["items"][0:2] = [{"name": "x"}, 42]
        assert len(doc["items"]) == 3
        assert doc["items"][0]["name"] == "a"
        assert doc["items"][1]["name"] == "b"

    def test_aot_setitem_slice_extended_rolls_back(self) -> None:
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
        with pytest.raises(TypeError, match="cannot append"):
            doc["items"][0:3:2] = [{"name": "x"}, 42]
        assert len(doc["items"]) == 3
        assert doc["items"][0]["name"] == "a"
        assert doc["items"][2]["name"] == "c"

    def test_aot_extend_rolls_back(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        with pytest.raises(TypeError, match="cannot append"):
            doc["items"].extend([{"name": "b"}, 42])
        assert len(doc["items"]) == 1
        assert doc["items"][0]["name"] == "a"

    # -- regular arrays --

    def test_array_setitem_slice_contiguous_rolls_back(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(TypeError, match="not a valid TOML value"):
            doc["arr"][0:2] = [10, None]
        assert list(doc["arr"]) == [1, 2, 3]

    def test_array_setitem_slice_extended_rolls_back(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(TypeError, match="not a valid TOML value"):
            doc["arr"][0:3:2] = [10, None]
        assert list(doc["arr"]) == [1, 2, 3]

    def test_array_extend_rolls_back(self) -> None:
        doc = Document.parse("arr = [1]\n")
        with pytest.raises(TypeError, match="not a valid TOML value"):
            doc["arr"].extend([2, None])
        assert list(doc["arr"]) == [1]
