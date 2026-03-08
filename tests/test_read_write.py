"""Tests for reading/writing scalars, arrays, keys, and format preservation."""

from __future__ import annotations

import zoneinfo
from datetime import datetime

import pytest

from tests.conftest import make_doc
from tomledit import Document

# ---------------------------------------------------------------------------
# Reading scalar values (like dict access)
# ---------------------------------------------------------------------------


class TestReadScalars:
    def test_top_level_string(self) -> None:
        doc = make_doc()
        assert doc["title"] == "Example"

    def test_nested_string(self) -> None:
        doc = make_doc()
        assert doc["owner"]["name"] == "Alice"

    def test_nested_int(self) -> None:
        doc = make_doc()
        assert doc["owner"]["age"] == 30

    def test_nested_bool(self) -> None:
        doc = make_doc()
        assert doc["owner"]["active"] == True  # noqa: E712

    def test_deeply_nested(self) -> None:
        doc = make_doc()
        assert doc["servers"]["alpha"]["ip"] == "10.0.0.1"
        assert doc["servers"]["beta"]["role"] == "backend"


# ---------------------------------------------------------------------------
# Reading array elements (like list access)
# ---------------------------------------------------------------------------


class TestReadArrays:
    def test_array_element_by_index(self) -> None:
        doc = make_doc()
        assert doc["database"]["ports"][0] == 8001
        assert doc["database"]["ports"][2] == 8002

    def test_array_out_of_bounds(self) -> None:
        doc = make_doc()
        with pytest.raises(IndexError):
            doc["database"]["ports"][99]


# ---------------------------------------------------------------------------
# Writing scalar values
# ---------------------------------------------------------------------------


class TestWriteScalars:
    def test_set_top_level_string(self) -> None:
        doc = make_doc()
        doc["title"] = "Changed"
        assert doc["title"] == "Changed"

    def test_set_nested_int(self) -> None:
        doc = make_doc()
        doc["owner"]["age"] = 31
        assert doc["owner"]["age"] == 31

    def test_set_nested_bool(self) -> None:
        doc = make_doc()
        doc["owner"]["active"] = False
        assert doc["owner"]["active"] == False  # noqa: E712

    def test_set_nested_float(self) -> None:
        doc = make_doc()
        doc["database"]["connection_max"] = 9.81
        assert doc["database"]["connection_max"] == 9.81

    def test_set_deeply_nested_string(self) -> None:
        doc = make_doc()
        doc["servers"]["alpha"]["ip"] = "192.168.1.1"
        assert doc["servers"]["alpha"]["ip"] == "192.168.1.1"

    def test_mutation_persists_in_str(self) -> None:
        doc = make_doc()
        doc["owner"]["name"] = "Bob"
        assert 'name = "Bob"' in str(doc)


# ---------------------------------------------------------------------------
# Writing array elements (like list assignment)
# ---------------------------------------------------------------------------


class TestWriteArrayElements:
    def test_replace_array_element(self) -> None:
        doc = make_doc()
        doc["database"]["ports"][0] = 9999
        assert doc["database"]["ports"][0] == 9999

    def test_replace_does_not_affect_others(self) -> None:
        doc = make_doc()
        doc["database"]["ports"][0] = 9999
        assert doc["database"]["ports"][2] == 8002


# ---------------------------------------------------------------------------
# Setting new keys (like dict[key] = value)
# ---------------------------------------------------------------------------


class TestSetNewKeys:
    def test_add_string(self) -> None:
        doc = Document.parse("")
        doc["name"] = "hello"
        assert doc["name"] == "hello"

    def test_add_int(self) -> None:
        doc = Document.parse("")
        doc["count"] = 42
        assert doc["count"] == 42

    def test_add_float(self) -> None:
        doc = Document.parse("")
        doc["gravity"] = 9.81
        assert doc["gravity"] == 9.81

    def test_add_bool(self) -> None:
        doc = Document.parse("")
        doc["flag"] = True
        assert doc["flag"] == True  # noqa: E712

    def test_add_list(self) -> None:
        doc = Document.parse("")
        doc["items"] = [1, 2, 3]
        assert doc["items"][0] == 1
        assert doc["items"][2] == 3

    def test_add_dict(self) -> None:
        doc = Document.parse("")
        doc["meta"] = {"key": "value"}
        assert doc["meta"]["key"] == "value"

    def test_add_nested_dict(self) -> None:
        doc = Document.parse("")
        doc["a"] = {"b": {"c": "deep"}}
        assert doc["a"]["b"]["c"] == "deep"

    def test_add_list_of_dicts(self) -> None:
        doc = Document.parse("")
        doc["entries"] = [{"x": 1}, {"x": 2}]
        assert doc["entries"][0]["x"] == 1
        assert doc["entries"][1]["x"] == 2

    def test_add_mixed_list(self) -> None:
        doc = Document.parse("")
        doc["mix"] = [1, "two", 3.0]
        assert doc["mix"][0] == 1
        assert doc["mix"][1] == "two"
        assert doc["mix"][2] == 3.0

    def test_add_datetime(self) -> None:
        doc = Document.parse("")
        chicago = zoneinfo.ZoneInfo("America/Chicago")
        now = datetime(2026, 1, 15, 12, 30, 0, tzinfo=chicago)
        doc["ts"] = now
        assert "2026-01-15" in str(doc["ts"])


# ---------------------------------------------------------------------------
# Chained mutation on array-of-tables
# ---------------------------------------------------------------------------


class TestArrayOfTablesMutation:
    def test_set_element_to_scalar(self) -> None:
        doc = Document.parse("")
        doc["d"] = [{"a": 1}, {"b": 2}]
        doc["d"][0] = 7
        assert doc["d"][0] == 7

    def test_set_nested_value_in_table(self) -> None:
        doc = Document.parse("")
        doc["d"] = [{"a": 1}, {"b": 2}]
        doc["d"][1]["b"] = 99
        assert doc["d"][1]["b"] == 99


# ---------------------------------------------------------------------------
# Chained mutation on inline tables (dicts)
# ---------------------------------------------------------------------------


class TestInlineTableMutation:
    def test_set_value_in_inline_table(self) -> None:
        doc = Document.parse("meta = {x = 1, y = 2}\n")
        doc["meta"]["x"] = 10
        assert doc["meta"]["x"] == 10

    def test_set_new_key_in_table(self) -> None:
        doc = make_doc()
        doc["owner"]["email"] = "alice@example.com"
        assert doc["owner"]["email"] == "alice@example.com"


# ---------------------------------------------------------------------------
# Format preservation
# ---------------------------------------------------------------------------


class TestFormatPreservation:
    def test_comments_preserved(self) -> None:
        toml = '# a comment\nkey = "value"\n'
        doc = Document.parse(toml)
        assert str(doc) == toml

    def test_whitespace_preserved(self) -> None:
        toml = 'key   =   "value"\n'
        doc = Document.parse(toml)
        assert str(doc) == toml

    def test_mutation_preserves_other_formatting(self) -> None:
        toml = "# header\na = 1\nb = 2\n"
        doc = Document.parse(toml)
        doc["a"] = 10
        result = str(doc)
        assert "# header" in result
        assert "b = 2" in result

    def test_inline_comment_preserved_on_top_level_update(self) -> None:
        toml = 'title = "old" # important note\n'
        doc = Document.parse(toml)
        doc["title"] = "new"
        assert "# important note" in str(doc)

    def test_inline_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\nname = "Tom"  # the owner name\nage = 30\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        result = str(doc)
        assert '"Bob"' in result
        assert "# the owner name" in result

    def test_standalone_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\n# this is the name\nname = "Tom"\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        result = str(doc)
        assert "# this is the name" in result
        assert '"Bob"' in result


# ---------------------------------------------------------------------------
# Mutation via .get(), .items(), .values() (not just __getitem__)
# ---------------------------------------------------------------------------


class TestMutationViaAccessors:
    def test_get_returns_live_proxy(self) -> None:
        doc = make_doc()
        owner = doc.get("owner")
        assert owner is not None
        owner["name"] = "Bob"
        assert doc["owner"]["name"] == "Bob"

    def test_items_returns_live_proxies(self) -> None:
        doc = make_doc()
        for key, proxy in doc.items():
            if key == "owner":
                proxy["name"] = "Charlie"
                break
        assert doc["owner"]["name"] == "Charlie"

    def test_values_returns_live_proxies(self) -> None:
        doc = Document.parse("[section]\nval = 10\n")
        vals = doc.values()
        assert len(vals) == 1
        vals[0]["val"] = 99
        assert doc["section"]["val"] == 99
