"""Tests for error handling: no panics, only proper Python exceptions."""

from __future__ import annotations

import pytest

from tomledit import Document


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
        with pytest.raises(TypeError, match=r"None.*not a valid TOML"):
            doc["x"] = None


# ---------------------------------------------------------------------------
# Parse errors
# ---------------------------------------------------------------------------


class TestParseErrors:
    def test_invalid_toml_raises_value_error(self) -> None:
        with pytest.raises(ValueError, match="cannot be empty"):
            Document.parse("[[[bad")

    def test_bare_value_raises(self) -> None:
        with pytest.raises(ValueError, match=r"expected|invalid"):
            Document.parse("= oops\n")

    def test_duplicate_key_raises(self) -> None:
        with pytest.raises(ValueError, match="duplicate"):
            Document.parse("x = 1\nx = 2\n")


# ---------------------------------------------------------------------------
# Unsupported Python types
# ---------------------------------------------------------------------------


class TestUnsupportedTypes:
    def test_assign_set_raises(self) -> None:
        """A set is not a valid TOML value and should raise an error."""
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match=r"[Cc]ould not convert|not a valid"):
            doc["x"] = {1, 2, 3}

    def test_assign_bytes_raises(self) -> None:
        """bytes has no meaningful TOML representation and should be rejected."""
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match="bytes"):
            doc["x"] = b"hi"

    def test_assign_complex_raises(self) -> None:
        doc = Document.parse("x = 1\n")
        with pytest.raises(TypeError, match=r"not a valid|convert"):
            doc["x"] = 3 + 4j


# ---------------------------------------------------------------------------
# get() on non-table type
# ---------------------------------------------------------------------------


class TestGetOnNonTable:
    def test_get_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        with pytest.raises(TypeError, match="get"):
            doc["arr"].get("x")

    def test_get_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="get"):
            doc["x"].get("y")


# ---------------------------------------------------------------------------
# Wrong-type error branches
# ---------------------------------------------------------------------------


class TestWrongTypeErrors:
    """Each method should raise TypeError when called on the wrong item type."""

    def test_pop_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="pop"):
            doc["x"].pop("y")

    def test_pop_noarg_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match=r"pop.*array"):
            doc["t"].pop()

    def test_update_on_array_raises(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        with pytest.raises(TypeError, match="update"):
            doc["arr"].update({"a": 1})

    def test_insert_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="insert"):
            doc["t"].insert(0, 99)

    def test_remove_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="remove"):
            doc["t"].remove(1)

    def test_extend_on_table_raises(self) -> None:
        doc = Document.parse("[t]\na = 1\n")
        with pytest.raises(TypeError, match="extend"):
            doc["t"].extend([1, 2])

    def test_del_inline_table_missing_key(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(KeyError):
            del doc["t"]["nonexistent"]

    def test_slice_del_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="slic"):
            del doc["x"][0:1]

    def test_slice_assign_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="slic"):
            doc["x"][0:1] = [1]

    def test_getitem_int_on_scalar_raises(self) -> None:
        doc = Document.parse("x = 42\n")
        with pytest.raises(TypeError, match="not subscriptable"):
            doc["x"][0]

    def test_pop_noarg_on_inline_table_raises(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        with pytest.raises(TypeError, match=r"pop.*array"):
            doc["t"].pop()
