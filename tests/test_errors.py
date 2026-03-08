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
        with pytest.raises(Exception, match=r"None.*not a valid TOML"):
            doc["x"] = None
