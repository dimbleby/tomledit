"""Tests for ScalarItem attribute forwarding via __getattr__."""

from __future__ import annotations

from datetime import date, time

import pytest

from tomledit import Document

SAMPLE = """\
name = "Alice"
port = 8080
rate = 3.14
enabled = true
birthday = 1990-05-15
alarm = 07:30:00
created = 1990-05-15T10:30:00Z
"""


@pytest.fixture
def doc() -> Document:
    return Document.parse(SAMPLE)


class TestStringForwarding:
    def test_upper(self, doc: Document) -> None:
        assert doc["name"].upper() == "ALICE"

    def test_lower(self, doc: Document) -> None:
        assert doc["name"].lower() == "alice"

    def test_startswith(self, doc: Document) -> None:
        assert doc["name"].startswith("Ali")
        assert not doc["name"].startswith("Bob")

    def test_endswith(self, doc: Document) -> None:
        assert doc["name"].endswith("ice")

    def test_replace(self, doc: Document) -> None:
        assert doc["name"].replace("Alice", "Bob") == "Bob"

    def test_split(self, doc: Document) -> None:
        assert doc["name"].split("l") == ["A", "ice"]

    def test_strip(self) -> None:
        doc2 = Document.parse('padded = "  hello  "')
        assert doc2["padded"].strip() == "hello"

    def test_title(self) -> None:
        doc2 = Document.parse('msg = "hello world"')
        assert doc2["msg"].title() == "Hello World"

    def test_isalpha(self, doc: Document) -> None:
        assert doc["name"].isalpha()


class TestIntForwarding:
    def test_bit_length(self, doc: Document) -> None:
        assert doc["port"].bit_length() == 13  # 8080 needs 13 bits

    def test_to_bytes(self, doc: Document) -> None:
        assert doc["port"].to_bytes(2, "big") == (8080).to_bytes(2, "big")


class TestFloatForwarding:
    def test_is_integer(self, doc: Document) -> None:
        assert not doc["rate"].is_integer()

    def test_as_integer_ratio(self, doc: Document) -> None:
        assert doc["rate"].as_integer_ratio() == (3.14).as_integer_ratio()

    def test_hex(self, doc: Document) -> None:
        assert doc["rate"].hex() == (3.14).hex()


class TestBoolForwarding:
    def test_bit_length(self, doc: Document) -> None:
        # bool is a subclass of int in Python
        assert doc["enabled"].bit_length() == 1


class TestDatetimeForwarding:
    def test_date_isoformat(self, doc: Document) -> None:
        assert doc["birthday"].isoformat() == "1990-05-15"

    def test_time_isoformat(self, doc: Document) -> None:
        assert doc["alarm"].isoformat() == "07:30:00"

    def test_datetime_isoformat(self, doc: Document) -> None:
        assert doc["created"].isoformat() == "1990-05-15T10:30:00+00:00"

    def test_date_year(self, doc: Document) -> None:
        assert doc["birthday"].year == 1990

    def test_date_month(self, doc: Document) -> None:
        assert doc["birthday"].month == 5

    def test_time_hour(self, doc: Document) -> None:
        assert doc["alarm"].hour == 7

    def test_datetime_date(self, doc: Document) -> None:
        assert doc["created"].date() == date(1990, 5, 15)

    def test_datetime_time(self, doc: Document) -> None:
        assert doc["created"].time() == time(10, 30)


class TestExistingAttrsNotShadowed:
    """Ensure Item-level attributes are not forwarded."""

    def test_value_property(self, doc: Document) -> None:
        assert doc["name"].value == "Alice"
        assert doc["port"].value == 8080

    def test_comment_property(self, doc: Document) -> None:
        # .comment should return the tomledit comment, not be forwarded
        assert doc["name"].comment is None

    def test_inline_comment_property(self, doc: Document) -> None:
        assert doc["name"].inline_comment is None

    def test_eq(self, doc: Document) -> None:
        assert doc["name"] == "Alice"
        assert doc["port"] == 8080

    def test_bool(self, doc: Document) -> None:
        assert bool(doc["name"]) is True


class TestErrorCases:
    def test_nonexistent_attr(self, doc: Document) -> None:
        with pytest.raises(AttributeError, match=r"ScalarItem.*str.*nonexistent"):
            _ = doc["name"].nonexistent

    def test_wrong_type_method(self, doc: Document) -> None:
        # int has no .upper()
        with pytest.raises(AttributeError, match=r"ScalarItem.*int.*upper"):
            doc["port"].upper()

    def test_stale_proxy(self, doc: Document) -> None:
        name = doc["name"]
        doc["name"] = "Bob"  # invalidates the proxy
        with pytest.raises(RuntimeError, match="stale"):
            name.upper()


class TestDictListNotAffected:
    """DictItem and ListItem should not gain forwarding."""

    def test_dict_no_forwarding(self) -> None:
        doc = Document.parse("[server]\nhost = 'x'\n")
        with pytest.raises(AttributeError):
            doc["server"].upper()

    def test_list_no_forwarding(self) -> None:
        doc = Document.parse("ports = [1, 2, 3]\n")
        with pytest.raises(AttributeError):
            doc["ports"].upper()
