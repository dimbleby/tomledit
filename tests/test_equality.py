"""Tests for equality semantics."""

from __future__ import annotations

from collections import OrderedDict
from collections.abc import Iterator, Mapping
from datetime import date, datetime, time, timedelta, timezone
from types import MappingProxyType
from typing import TYPE_CHECKING

import pytest
from typing_extensions import override

from tests.conftest import toml_literal
from tomledit import Document

if TYPE_CHECKING:
    from collections.abc import Iterator

# ---------------------------------------------------------------------------
# Equality semantics
# ---------------------------------------------------------------------------


class TestEquality:
    def test_int_eq(self, doc: Document) -> None:
        assert doc["owner"]["age"] == 30
        assert doc["owner"]["age"] != 31

    def test_str_eq(self, doc: Document) -> None:
        assert doc["owner"]["name"] == "Alice"
        assert doc["owner"]["name"] != "Bob"

    def test_bool_eq(self, doc: Document) -> None:
        assert doc["owner"]["active"] == True  # noqa: E712
        assert doc["owner"]["active"] != False  # noqa: E712

    def test_float_eq(self) -> None:
        doc = Document.parse("val = 2.5\n")
        assert doc["val"] == 2.5

    def test_type_mismatch_not_equal(self, doc: Document) -> None:
        assert doc["owner"]["age"] != "30"
        assert doc["owner"]["name"] != 42

    def test_datetime_proxy_eq(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        expected = datetime(2024, 1, 15, 10, 30, 0, tzinfo=timezone.utc)
        assert doc["dt"] == expected

    def test_datetime_ne(self) -> None:
        doc = Document.parse("dt = 2024-01-15T10:30:00Z\n")
        wrong = datetime(2000, 1, 1, 0, 0, 0, tzinfo=timezone.utc)
        assert doc["dt"] != wrong

    def test_array_equals_list(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] == [1, 2, 3]

    def test_array_not_equals_different_list(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] != [1, 2, 4]

    def test_array_not_equals_different_length(self) -> None:
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"] != [1, 2]

    def test_empty_array_equals_empty_list(self) -> None:
        doc = Document.parse("arr = []\n")
        assert doc["arr"] == []

    def test_table_equals_dict(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        assert doc["t"] == {"a": 1, "b": 2}

    def test_table_not_equals_different_dict(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        assert doc["t"] != {"a": 2}

    def test_inline_table_equals_dict(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert doc["meta"] == {"x": 1, "y": 2}

    def test_inline_table_eq_mapping_proxy(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        assert doc["meta"] == MappingProxyType({"x": 1, "y": 2})

    def test_string_array_equals_list(self) -> None:
        doc = Document.parse('arr = ["a", "b"]\n')
        assert doc["arr"] == ["a", "b"]

    def test_reverse_equality(self, doc: Document) -> None:
        """Python falls back to proxy's __eq__ in both directions."""
        assert 30 == doc["owner"]["age"]  # noqa: SIM300
        assert [8001, 8001, 8002] == doc["database"]["ports"]  # noqa: SIM300

    def test_bool_not_equal_to_int(self) -> None:
        """TOML types are strict: bool != int, even though Python's True == 1."""
        doc = Document.parse(
            toml_literal("""
            count = 1
            flag = true
        """)
        )
        assert doc["count"] != True  # noqa: E712
        assert doc["flag"] != 1

    def test_bool_not_equal_to_float(self) -> None:
        doc = Document.parse("val = 1.0\n")
        assert doc["val"] != True  # noqa: E712

    def test_proxy_self_equality(self) -> None:
        doc = Document.parse("a = 42\n")
        assert doc["a"] == doc["a"]

    def test_proxy_vs_proxy_nested(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t1]
            x = 1

            [t2]
            x = 1
        """)
        )
        assert doc["t1"] == doc["t2"]

    def test_proxy_vs_proxy_type_strict(self) -> None:
        """Proxy-vs-proxy should preserve TOML type strictness."""
        doc = Document.parse(
            toml_literal("""
            count = 1
            flag = true
        """)
        )
        assert doc["count"] != doc["flag"]


# ---------------------------------------------------------------------------
# Proxy-vs-proxy structural equality for various types
# ---------------------------------------------------------------------------


class TestProxyStructuralEquality:
    """Exercise values_structural_eq for types beyond integers."""

    def test_proxy_vs_proxy_bool(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = true
            b = true
        """)
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_float(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 1.5
            b = 1.5
        """)
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_string(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = "hi"
            b = "hi"
        """)
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_datetime(self) -> None:
        doc = Document.parse(
            toml_literal("""
                a = 2024-01-15T10:30:00Z
                b = 2024-01-15T10:30:00+00:00
            """)
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_date_only(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 2024-01-15
            b = 2024-01-15
        """)
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_date_only_different(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = 2024-01-15
            b = 2025-06-01
        """)
        )
        assert doc["a"] != doc["b"]

    def test_proxy_vs_proxy_array(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = [1, 2, 3]
            b = [1, 2, 3]
        """)
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_inline_table(self) -> None:
        doc = Document.parse(
            toml_literal("""
            a = {x = 1}
            b = {x = 1}
        """)
        )
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_array_of_tables(self) -> None:
        toml = toml_literal("""
            [[a]]
            x = 1
            [[a]]
            x = 2
            [[b]]
            x = 1
            [[b]]
            x = 2
        """)
        doc = Document.parse(toml)
        assert doc["a"] == doc["b"]

    def test_proxy_vs_proxy_different_types(self) -> None:
        """Table vs ArrayOfTables should not be equal."""
        doc = Document.parse(
            toml_literal("""
            [a]
            x = 1
            [[b]]
            x = 1
        """)
        )
        assert doc["a"] != doc["b"]

    def test_list_index_with_scalar_proxy(self) -> None:
        """list.index(proxy) exercises value_eq's proxy fast-path."""
        doc = Document.parse("arr = [1, 2, 3]\nx = 2\n")
        assert doc["arr"].index(doc["x"]) == 1

    def test_list_count_with_table_proxy(self) -> None:
        """list.count(proxy) on AOT exercises table_eq's proxy fast-path."""
        doc = Document.parse("[[items]]\nx = 1\n[[items]]\nx = 2\n")
        ref = Document.parse("[t]\nx = 1\n")
        assert doc["items"].count(ref["t"]) == 1

    def test_list_count_proxy_type_mismatch(self) -> None:
        """value_eq with a table proxy (or vice versa) returns false."""
        doc = Document.parse("arr = [1, 2]\n[t]\nx = 1\n")
        assert doc["arr"].count(doc["t"]) == 0

    def test_table_vs_inline_table_proxy(self) -> None:
        """Table and InlineTable proxies with same content should be equal."""
        doc = Document.parse(
            toml_literal("""
            b = {x = 1}
            [a]
            x = 1
        """)
        )
        assert doc["a"] == doc["b"]
        assert doc["b"] == doc["a"]

    def test_aot_vs_array_of_inline_tables_proxy(self) -> None:
        """AoT and array-of-inline-tables proxies should be equal."""
        doc = Document.parse(
            toml_literal("""
            b = [{x = 1}, {x = 2}]
            [[a]]
            x = 1
            [[a]]
            x = 2
        """)
        )
        assert doc["a"] == doc["b"]
        assert doc["b"] == doc["a"]

    def test_table_vs_inline_table_in_list_count(self) -> None:
        """table_eq proxy fast path: Table in AoT counted against InlineTable proxy."""
        aot_doc = Document.parse("[[items]]\nx = 1\n[[items]]\nx = 2\n")
        inline_doc = Document.parse("t = {x = 1}\n")
        assert aot_doc["items"].count(inline_doc["t"]) == 1

    def test_inline_table_vs_table_in_list_count(self) -> None:
        """value_eq proxy fast path: InlineTable counted against Table."""
        doc = Document.parse("arr = [{x = 1}, {x = 2}]\n[t]\nx = 1\n")
        assert doc["arr"].count(doc["t"]) == 1

    def test_table_vs_inline_table_nested_aot(self) -> None:
        """Nested AoT inside Table vs nested array inside InlineTable."""
        doc = Document.parse(
            toml_literal("""
            b = {items = [{x = 1}]}
            [a]
            [[a.items]]
            x = 1
        """)
        )
        assert doc["a"] == doc["b"]
        assert doc["b"] == doc["a"]


# ---------------------------------------------------------------------------
# Equality edge cases
# ---------------------------------------------------------------------------


class TestEqualityEdgeCases:
    def test_table_extra_key_not_equal(self) -> None:
        """Table with keys {a, b} should != dict with keys {a, c}."""
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
            b = 2
        """)
        )
        assert doc["t"] != {"a": 1, "c": 2}

    def test_table_vs_non_dict(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [t]
            a = 1
        """)
        )
        assert doc["t"] != [1, 2]
        assert doc["t"] != "hello"
        assert doc["t"] != 42

    def test_inline_table_length_mismatch(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        assert doc["t"] != {"a": 1}

    def test_inline_table_key_mismatch(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        assert doc["t"] != {"a": 1, "c": 2}

    def test_inline_table_value_mismatch(self) -> None:
        doc = Document.parse("t = {a = 1, b = 2}\n")
        assert doc["t"] != {"a": 1, "b": 99}

    def test_inline_table_vs_non_dict(self) -> None:
        doc = Document.parse("t = {a = 1}\n")
        assert doc["t"] != [1]

    def test_aot_equality_with_list_of_dicts(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                value = 1
                [[items]]
                name = "b"
                value = 2
            """)
        )
        expected = [{"name": "a", "value": 1}, {"name": "b", "value": 2}]
        assert doc["items"] == expected

    def test_aot_inequality_with_wrong_list(self) -> None:
        doc = Document.parse(
            toml_literal("""
                [[items]]
                name = "a"
                value = 1
                [[items]]
                name = "b"
                value = 2
            """)
        )
        assert doc["items"] != [{"name": "a", "value": 1}]

    def test_aot_inequality_with_non_list(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            value = 1
        """)
        )
        assert doc["items"] != "not a list"

    def test_aot_entry_value_mismatch(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            name = "b"
        """)
        )
        assert doc["items"] != [{"name": "a"}, {"name": "WRONG"}]

    def test_aot_entry_non_dict_in_list(self) -> None:
        doc = Document.parse(
            toml_literal("""
            [[items]]
            name = "a"
        """)
        )
        assert doc["items"] != ["not a dict"]

    def test_float_not_equal_to_string(self) -> None:
        doc = Document.parse("val = 1.5\n")
        assert doc["val"] != "1.5"

    def test_date_only_not_equal_to_string(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert doc["d"] != "2024-01-15"

    def test_full_datetime_not_equal_to_date(self) -> None:
        """A TOML datetime (with time component) should not equal a Python date."""
        doc = Document.parse("dt = 2024-01-15T10:30:00\n")
        assert doc["dt"] != date(2024, 1, 15)

    def test_full_datetime_not_equal_to_time(self) -> None:
        """A TOML datetime (with date component) should not equal a Python time."""
        doc = Document.parse("dt = 2024-01-15T10:30:00\n")
        assert doc["dt"] != time(10, 30, 0)

    def test_array_not_equal_to_string(self) -> None:
        doc = Document.parse("arr = [1, 2]\n")
        assert doc["arr"] != "not a list"

    def test_bool_not_equal_to_string(self) -> None:
        doc = Document.parse("flag = true\n")
        assert doc["flag"] != "true"

    def test_date_only_equals_python_date(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert doc["d"] == date(2024, 1, 15)

    def test_date_only_not_equal_to_different_date(self) -> None:
        doc = Document.parse("d = 2024-01-15\n")
        assert doc["d"] != date(2025, 6, 1)

    def test_time_only_not_equal_to_different_time(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        assert doc["t"] != time(11, 0, 0)

    def test_reverse_time_equality(self) -> None:
        doc = Document.parse("t = 10:30:00\n")
        assert time(10, 30, 0) == doc["t"]

    def test_datetime_equality_with_non_utc_offset(self) -> None:
        """Proxy datetime == Python datetime with matching non-UTC offset."""
        doc = Document.parse("dt = 2024-01-15T10:30:00+05:30\n")
        tz = timezone(timedelta(hours=5, minutes=30))
        expected = datetime(2024, 1, 15, 10, 30, 0, tzinfo=tz)
        assert doc["dt"] == expected

    def test_datetime_equality_same_instant_different_offset(self) -> None:
        """Aware datetimes that represent the same instant should compare equal."""
        doc = Document.parse("dt = 2024-01-15T10:30:00+01:00\n")
        expected = datetime(2024, 1, 15, 9, 30, 0, tzinfo=timezone.utc)
        assert doc["dt"] == expected

    def test_proxy_date_only_ne_proxy_full_datetime(self) -> None:
        """Proxy-vs-proxy: date-only != full datetime even with same date."""
        doc = Document.parse(
            toml_literal("""
            d = 2024-01-15
            dt = 2024-01-15T10:30:00Z
        """)
        )
        assert doc["d"] != doc["dt"]

    def test_proxy_time_only_ne_proxy_full_datetime(self) -> None:
        """Proxy-vs-proxy: time-only != full datetime even with same time."""
        doc = Document.parse(
            toml_literal("""
            t = 10:30:00
            dt = 2024-01-15T10:30:00
        """)
        )
        assert doc["t"] != doc["dt"]


class TestNumericTowerEquality:
    """Python's numeric tower: 1 == 1.0 == True.

    TOML integer and float proxies should follow Python semantics when compared
    to Python objects, even across numeric types.
    """

    def test_int_proxy_eq_python_float(self) -> None:
        doc = Document.parse("x = 1\n")
        assert doc["x"] == 1.0

    def test_float_proxy_eq_python_int(self) -> None:
        doc = Document.parse("x = 1.0\n")
        assert doc["x"] == 1

    def test_float_in_int_array(self) -> None:
        """1.0 in [1, 2, 3] is True in Python."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert 1.0 in doc["arr"]

    def test_int_in_float_array(self) -> None:
        """1 in [1.0, 2.0] is True in Python."""
        doc = Document.parse("arr = [1.0, 2.0]\n")
        assert 1 in doc["arr"]

    def test_cross_type_proxy_eq(self) -> None:
        """Two proxies: int 1 vs float 1.0 — cross-type numeric equality."""
        doc = Document.parse(
            toml_literal("""
            x = 1
            y = 1.0
        """)
        )
        assert doc["x"] == doc["y"]

    def test_large_int_proxy_not_equal_to_rounded_python_float(self) -> None:
        """Large integers should not compare equal to rounded IEEE-754 floats."""
        doc = Document.parse("x = 9007199254740993\n")
        assert doc["x"] != float(9007199254740993)

    def test_float_proxy_not_equal_to_nearby_large_python_int(self) -> None:
        """A float should not compare equal to an int it can't exactly represent."""
        doc = Document.parse("x = 9007199254740992.0\n")
        assert doc["x"] != 9007199254740993


class TestTimeZoneEquality:
    """TOML local time must NOT equal a timezone-aware Python time."""

    def test_local_time_not_equal_to_aware_time(self) -> None:
        doc = Document.parse("t = 12:30:15\n")
        aware = time(12, 30, 15, tzinfo=timezone.utc)
        assert doc["t"] != aware

    def test_local_time_not_equal_to_aware_time_reverse(self) -> None:
        doc = Document.parse("t = 12:30:15\n")
        aware = time(12, 30, 15, tzinfo=timezone.utc)
        assert aware != doc["t"]

    def test_local_time_equal_to_naive_time(self) -> None:
        """Sanity check: local time DOES equal a naive time with same fields."""
        doc = Document.parse("t = 12:30:15\n")
        naive = time(12, 30, 15)
        assert doc["t"] == naive


class TestMappingEquality:
    """Documents and DictItems should compare equal to any Mapping, not just dict."""

    def test_document_eq_mapping_proxy(self) -> None:
        doc = Document({"a": 1, "b": "hello"})
        assert doc == MappingProxyType({"a": 1, "b": "hello"})

    def test_document_ne_mapping_proxy(self) -> None:
        doc = Document({"a": 1})
        assert doc != MappingProxyType({"a": 2})

    def test_document_ne_mapping_proxy_extra_key(self) -> None:
        doc = Document({"a": 1})
        assert doc != MappingProxyType({"a": 1, "b": 2})

    def test_dict_item_eq_mapping_proxy(self) -> None:
        doc = Document.parse("[section]\na = 1\nb = 2\n")
        assert doc["section"] == MappingProxyType({"a": 1, "b": 2})

    def test_dict_item_ne_mapping_proxy(self) -> None:
        doc = Document.parse("[section]\na = 1\n")
        assert doc["section"] != MappingProxyType({"a": 99})

    def test_document_eq_ordered_dict(self) -> None:
        doc = Document({"x": 10, "y": 20})
        assert doc == OrderedDict({"x": 10, "y": 20})

    def test_document_eq_custom_mapping(self) -> None:
        class MyMap(Mapping[str, object]):
            def __init__(self, d: dict[str, object]) -> None:
                self._d = d

            @override
            def __getitem__(self, key: str) -> object:
                return self._d[key]

            @override
            def __iter__(self) -> Iterator[str]:
                return iter(self._d)

            @override
            def __len__(self) -> int:
                return len(self._d)

        doc = Document({"k": "v"})
        assert doc == MyMap({"k": "v"})

    def test_document_ne_non_mapping(self) -> None:
        doc = Document({"a": 1})
        assert doc != [("a", 1)]


# ---------------------------------------------------------------------------
# Document as equality operand — lock safety (regression tests)
# ---------------------------------------------------------------------------


class TestDocumentAsEqualityOperand:
    """Equality with a Document operand must use lock-reuse, not Python callbacks.

    Without the lock-reuse fast path for Document objects, these comparisons
    would recursively acquire (or write-then-read) the same RwLock, causing
    undefined behaviour or guaranteed deadlock.
    """

    def test_proxy_eq_same_document(self) -> None:
        """ItemProxy.__eq__(own Document) — recursive read lock."""
        doc = Document.parse("[server]\nport = 8080\n")
        assert doc["server"] != doc

    def test_proxy_eq_different_document_equal(self) -> None:
        """ItemProxy.__eq__(other Document) — cross-document, equal content."""
        doc = Document.parse("[server]\nport = 8080\n")
        other = Document({"port": 8080})
        assert doc["server"] == other

    def test_proxy_eq_different_document_not_equal(self) -> None:
        doc = Document.parse("[server]\nport = 8080\n")
        other = Document({"port": 9090})
        assert doc["server"] != other

    def test_list_remove_same_document(self) -> None:
        """ListProxy.remove(own Document) — write-then-read GUARANTEED deadlock."""
        doc = Document.parse("arr = [{a = 1}]\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].remove(doc)

    def test_list_remove_different_document(self) -> None:
        """ListProxy.remove(other Document) — cross-document, matching content."""
        doc = Document.parse("arr = [{a = 1}, {b = 2}]\n")
        other = Document({"a": 1})
        doc["arr"].remove(other)
        assert len(doc["arr"]) == 1
        assert doc["arr"][0] == {"b": 2}

    def test_list_contains_same_document(self) -> None:
        """ListProxy.__contains__(own Document) — recursive read lock."""
        doc = Document.parse("arr = [{a = 1}]\n")
        assert doc not in doc["arr"]

    def test_list_contains_different_document(self) -> None:
        """A different Document whose root matches an array element."""
        doc = Document.parse("arr = [{a = 1}]\n")
        other = Document({"a": 1})
        assert other in doc["arr"]

    def test_list_count_same_document(self) -> None:
        """ListProxy.count(own Document) — recursive read lock."""
        doc = Document.parse("arr = [{a = 1}]\n")
        assert doc["arr"].count(doc) == 0

    def test_list_count_different_document(self) -> None:
        doc = Document.parse("arr = [{a = 1}]\n")
        other = Document({"a": 1})
        assert doc["arr"].count(other) == 1

    def test_list_index_same_document(self) -> None:
        """ListProxy.index(own Document) — recursive read lock."""
        doc = Document.parse("arr = [{a = 1}]\n")
        with pytest.raises(ValueError, match="not in array"):
            doc["arr"].index(doc)

    def test_list_index_different_document(self) -> None:
        doc = Document.parse("arr = [{a = 1}]\n")
        other = Document({"a": 1})
        assert doc["arr"].index(other) == 0

    def test_values_view_contains_same_document(self) -> None:
        """ValuesView.__contains__(own Document) — recursive read lock."""
        doc = Document.parse(
            toml_literal("""
            [section]
            key = 1
        """)
        )
        assert doc not in doc.values()

    def test_items_view_contains_same_document(self) -> None:
        """ItemsView.__contains__ with (key, Document) tuple."""
        doc = Document.parse(
            toml_literal("""
            [section]
            key = 1
        """)
        )
        assert ("section", doc) not in doc.items()  # type: ignore[comparison-overlap]

    def test_document_eq_document_else_branch(self) -> None:
        """Document.__eq__ else branch (non-Document other) still works."""
        doc = Document({"a": 1, "b": 2})
        assert doc == {"a": 1, "b": 2}
        assert doc != {"a": 1, "c": 3}

    def test_aot_contains_different_document(self) -> None:
        """AoT element_eq → table_eq → with_doc_item_ctx."""
        doc = Document.parse("[[arr]]\na = 1\n")
        other = Document({"a": 1})
        assert other in doc["arr"]

    def test_aot_contains_same_document(self) -> None:
        doc = Document.parse("[[arr]]\na = 1\n")
        assert doc not in doc["arr"]

    def test_aot_count_different_document(self) -> None:
        doc = Document.parse("[[arr]]\na = 1\n[[arr]]\na = 2\n")
        other = Document({"a": 1})
        assert doc["arr"].count(other) == 1

    def test_aot_index_different_document(self) -> None:
        doc = Document.parse("[[arr]]\na = 1\n[[arr]]\na = 2\n")
        other = Document({"a": 2})
        assert doc["arr"].index(other) == 1

    def test_aot_remove_different_document(self) -> None:
        doc = Document.parse("[[arr]]\na = 1\n[[arr]]\na = 2\n")
        other = Document({"a": 1})
        doc["arr"].remove(other)
        assert len(doc["arr"]) == 1
        assert doc["arr"][0] == {"a": 2}

    def test_inline_array_contains_different_document(self) -> None:
        """Array of inline tables → value_eq → with_doc_item_ctx."""
        doc = Document.parse("arr = [{a = 1}]\n")
        other = Document({"a": 1})
        assert other in doc["arr"]

    def test_document_eq_self_identity(self) -> None:
        """Document.__eq__ same-object identity fast path."""
        doc = Document({"a": 1})
        assert doc == doc


class TestEqualityErrorPropagation:
    """Non-TypeError exceptions from extraction must propagate, not silently
    return False."""

    def test_eq_propagates_mapping_error(self) -> None:
        msg = "boom"

        class BadMapping(Mapping[str, object]):
            @override
            def __getitem__(self, key: str) -> object:
                raise RuntimeError(msg)

            @override
            def __iter__(self) -> Iterator[str]:
                raise RuntimeError(msg)

            @override
            def __len__(self) -> int:
                return 1

        doc = Document({"a": 1})
        with pytest.raises(RuntimeError, match="boom"):
            assert doc == BadMapping()

    def test_contains_propagates_mapping_error(self) -> None:
        msg = "boom"

        class BadMapping(Mapping[str, object]):
            @override
            def __getitem__(self, key: str) -> object:
                raise RuntimeError(msg)

            @override
            def __iter__(self) -> Iterator[str]:
                raise RuntimeError(msg)

            @override
            def __len__(self) -> int:
                return 1

        doc = Document.parse("arr = [{a = 1}]\n")
        with pytest.raises(RuntimeError, match="boom"):
            assert BadMapping() in doc["arr"]
