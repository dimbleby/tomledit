"""Tests for the module-level load / loads / dump / dumps helpers."""

from __future__ import annotations

import io
from typing import TYPE_CHECKING

import pytest

import tomledit
from tests.conftest import SAMPLE, toml_literal
from tomledit import Document

if TYPE_CHECKING:
    from collections.abc import Mapping


class TestLoads:
    def test_round_trip_preserves_formatting(self) -> None:
        doc = tomledit.loads(SAMPLE)
        assert isinstance(doc, Document)
        assert doc.as_toml() == SAMPLE

    def test_returns_document(self) -> None:
        doc = tomledit.loads("a = 1\n")
        assert isinstance(doc, Document)
        assert doc["a"] == 1

    def test_invalid_raises_value_error(self) -> None:
        with pytest.raises(ValueError, match="expected"):
            tomledit.loads("a = =")


class TestLoad:
    def test_round_trip(self) -> None:
        buf = io.BytesIO(SAMPLE.encode("utf-8"))
        doc = tomledit.load(buf)
        assert isinstance(doc, Document)
        assert doc.as_toml() == SAMPLE

    def test_text_mode_rejected(self) -> None:
        with pytest.raises(TypeError, match="binary mode"):
            tomledit.load(io.StringIO("a = 1\n"))  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]

    def test_invalid_utf8_rejected(self) -> None:
        with pytest.raises(UnicodeDecodeError):
            tomledit.load(io.BytesIO(b"a = \xff\xfe"))

    def test_invalid_toml_raises_value_error(self) -> None:
        with pytest.raises(ValueError, match="expected"):
            tomledit.load(io.BytesIO(b"a = ="))


class TestDumps:
    def test_document_preserves_formatting(self) -> None:
        doc = tomledit.loads(SAMPLE)
        assert tomledit.dumps(doc) == SAMPLE

    def test_mapping_serialises(self) -> None:
        out = tomledit.dumps({"a": 1, "b": "two"})
        assert out == toml_literal("""
            a = 1
            b = "two"
        """)

    def test_mapping_with_nested_table(self) -> None:
        out = tomledit.dumps({"section": {"key": "value"}})
        assert out == toml_literal("""
            [section]
            key = "value"
        """)

    def test_non_mapping_raises_type_error(self) -> None:
        with pytest.raises(TypeError):
            tomledit.dumps(42)  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]

    def test_dict_subclass_accepted(self) -> None:
        class MyDict(dict[str, object]):
            pass

        out = tomledit.dumps(MyDict({"x": 1}))
        assert out == "x = 1\n"


class TestDump:
    def test_writes_utf8_bytes(self) -> None:
        doc = tomledit.loads(SAMPLE)
        buf = io.BytesIO()
        tomledit.dump(doc, buf)
        assert buf.getvalue() == SAMPLE.encode("utf-8")

    def test_dump_mapping(self) -> None:
        buf = io.BytesIO()
        tomledit.dump({"a": 1}, buf)
        assert buf.getvalue() == b"a = 1\n"

    def test_round_trip_via_files(self) -> None:
        doc = tomledit.loads(SAMPLE)
        buf = io.BytesIO()
        tomledit.dump(doc, buf)
        buf.seek(0)
        round_tripped = tomledit.load(buf)
        assert round_tripped.as_toml() == SAMPLE

    def test_text_mode_rejected(self) -> None:
        with pytest.raises(TypeError, match="binary mode"):
            tomledit.dump({"a": 1}, io.StringIO())  # type: ignore[arg-type]  # ty: ignore[invalid-argument-type]

    def test_unicode_round_trip(self) -> None:
        original: Mapping[str, object] = {"greeting": "héllo 🌍"}
        buf = io.BytesIO()
        tomledit.dump(original, buf)
        buf.seek(0)
        doc = tomledit.load(buf)
        assert doc["greeting"] == "héllo 🌍"
