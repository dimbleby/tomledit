"""Tests for error handling: no panics, only proper Python exceptions."""

from __future__ import annotations

import pytest

from tomledit import Document

# ---------------------------------------------------------------------------
# Parse errors
# ---------------------------------------------------------------------------


class TestParseErrors:
    def test_invalid_toml_raises_value_error(self) -> None:
        with pytest.raises(ValueError, match="cannot be empty"):
            Document.parse("[[[bad")

    def test_missing_key_raises(self) -> None:
        with pytest.raises(ValueError, match=r"expected|invalid"):
            Document.parse("= oops\n")

    def test_duplicate_key_raises(self) -> None:
        with pytest.raises(ValueError, match="duplicate"):
            Document.parse("x = 1\nx = 2\n")


# ---------------------------------------------------------------------------
# Unsupported Python types assigned to TOML keys
# ---------------------------------------------------------------------------


class TestUnsupportedValueTypes:
    """Python types without obvious TOML semantics should be rejected."""

    def test_none_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="None is not a valid TOML value"):
            doc["x"] = None

    def test_bytes_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError):
            doc["x"] = b"hello"

    def test_bytearray_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError):
            doc["x"] = bytearray(b"hello")

    def test_set_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError):
            doc["x"] = {1, 2, 3}

    def test_range_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError):
            doc["x"] = range(5)

    def test_complex_rejected(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match=r"not a valid|convert"):
            doc["x"] = 3 + 4j


# ---------------------------------------------------------------------------
# Operations on wrong item types
# ---------------------------------------------------------------------------


class TestWrongTypeErrors:
    """Each method should raise TypeError when called on the wrong item type."""

    # -- subscript on scalars --

    def test_setitem_on_string_raises(self) -> None:
        doc = Document.parse('version = "0.0.1"\n')
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["version"][3] = 12

    def test_setitem_on_int_raises(self) -> None:
        doc = Document.parse("count = 42\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["count"][0] = 1

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
        with pytest.raises(TypeError, match="does not support slicing"):
            del doc["x"][0:1]

    def test_slice_assign_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="does not support slicing"):
            doc["x"][0:1] = [1]

    # -- wrong key type --

    def test_getitem_float_key_raises(self) -> None:
        doc = Document.parse("[tbl]\nx = 1\n")
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            doc["tbl"][1.5]  # type: ignore[call-overload]

    def test_setitem_int_key_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="strings, not integers"):
            doc["t"][0] = 99

    def test_setitem_str_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="integers, not strings"):
            doc["arr"]["x"] = 99

    def test_delitem_int_key_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="strings, not integers"):
            del doc["t"][0]

    def test_delitem_str_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="integers, not strings"):
            del doc["arr"]["x"]

    def test_setitem_float_key_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            doc["t"][1.5] = 99  # type: ignore[index]

    def test_delitem_float_key_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="indices must be integers or strings"):
            del doc["arr"][1.5]  # type: ignore[arg-type]

    # -- missing keys --

    def test_getitem_missing_nested_key_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(KeyError, match="nonexistent"):
            doc["t"]["nonexistent"]

    def test_delitem_inline_table_missing_key_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(KeyError):
            del doc["t"]["nonexistent"]

    # -- dict methods on wrong types --

    def test_keys_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(AttributeError):
            doc["arr"].keys()

    def test_get_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(AttributeError):
            doc["arr"].get("x")

    def test_get_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(AttributeError):
            doc["x"].get("y")

    def test_pop_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(AttributeError):
            doc["x"].pop("y")

    def test_pop_noarg_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="missing 1 required positional argument"):
            doc["t"].pop()

    def test_pop_noarg_on_inline_table_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(TypeError, match="missing 1 required positional argument"):
            doc["t"].pop()

    def test_update_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(AttributeError):
            doc["arr"].update({"a": 1})

    # -- list methods on wrong types --

    def test_insert_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(AttributeError):
            doc["t"].insert(0, 99)

    def test_remove_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(AttributeError):
            doc["t"].remove(1)

    def test_extend_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(AttributeError):
            doc["t"].extend([1, 2])

    # -- view methods on wrong types --

    def test_keys_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(AttributeError):
            doc["x"].keys()

    def test_values_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(AttributeError):
            doc["x"].values()

    def test_items_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(AttributeError):
            doc["x"].items()
