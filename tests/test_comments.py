"""Tests for the .comment (block) and .inline_comment (inline) properties."""

from __future__ import annotations

import pytest

from tomledit import Document


class TestComment:
    # ---- reading inline comment ----

    def test_read_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # inline note\n")
        assert doc["key"].inline_comment == "# inline note"

    def test_read_no_inline_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        assert doc["key"].inline_comment is None

    def test_read_inline_on_string(self) -> None:
        doc = Document.parse('name = "Alice" # person\n')
        assert doc["name"].inline_comment == "# person"

    def test_read_inline_on_table_value(self) -> None:
        doc = Document.parse("[section]\nx = 1 # inside\n")
        assert doc["section"]["x"].inline_comment == "# inside"

    # ---- setting inline comment ----

    def test_set_inline_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "# added"
        assert "# added" in str(doc)
        assert doc["key"].inline_comment == "# added"

    def test_replace_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # old\n")
        doc["key"].inline_comment = "# new"
        assert doc["key"].inline_comment == "# new"
        assert "# old" not in str(doc)

    def test_clear_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # remove me\n")
        doc["key"].inline_comment = None
        assert doc["key"].inline_comment is None
        assert "#" not in str(doc)

    def test_inline_comment_preserves_value(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "# note"
        assert doc["key"] == 42

    # ---- reading comment ----

    def test_read_comment(self) -> None:
        doc = Document.parse("# before\nkey = 42\n")
        assert doc["key"].comment == "# before"

    def test_read_no_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        assert doc["key"].comment is None

    def test_read_multiline_comment(self) -> None:
        doc = Document.parse("# line 1\n# line 2\nkey = 42\n")
        assert doc["key"].comment == "# line 1\n# line 2"

    def test_read_comment_nested(self) -> None:
        doc = Document.parse("[section]\n# before x\nx = 1\n")
        assert doc["section"]["x"].comment == "# before x"

    # ---- setting comment ----

    def test_set_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# added"
        assert "# added" in str(doc)
        assert doc["key"].comment == "# added"

    def test_replace_comment(self) -> None:
        doc = Document.parse("# old\nkey = 42\n")
        doc["key"].comment = "# new"
        assert doc["key"].comment == "# new"
        assert "# old" not in str(doc)

    def test_clear_comment(self) -> None:
        doc = Document.parse("# remove\nkey = 42\n")
        doc["key"].comment = None
        assert doc["key"].comment is None

    def test_multiline_comment_set(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# line A\n# line B"
        s = str(doc)
        assert "# line A\n# line B\n" in s
        assert doc["key"].comment == "# line A\n# line B"

    # ---- both together ----

    def test_both_comments(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# above"
        doc["key"].inline_comment = "# inline"
        s = str(doc)
        assert "# above" in s
        assert "# inline" in s
        assert doc["key"].comment == "# above"
        assert doc["key"].inline_comment == "# inline"

    # ---- round-trip ----

    def test_comment_roundtrip(self) -> None:
        toml = "# important\nkey = 42 # magic number\n"
        doc = Document.parse(toml)
        assert str(doc) == toml

    # ---- array element comments ----

    def test_comment_on_array_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        assert doc["arr"][0].comment is None
        doc["arr"][1].comment = "# about two"
        s = str(doc)
        assert "# about two" in s
        assert "  # about two\n  2" in s

    def test_set_inline_comment_on_array_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][2].inline_comment = "# last"
        s = str(doc)
        assert "1, # first" in s
        assert "3, # last" in s
        doc2 = Document.parse(s)
        assert doc2["arr"][0].inline_comment == "# first"
        assert doc2["arr"][1].inline_comment is None
        assert doc2["arr"][2].inline_comment == "# last"

    def test_array_inline_comment_roundtrip(self) -> None:
        toml = 'arr = [\n  "a", # one\n  "b", # two\n  "c", # three\n]\n'
        doc = Document.parse(toml)
        assert str(doc) == toml
        assert doc["arr"][0].inline_comment == "# one"
        assert doc["arr"][1].inline_comment == "# two"
        assert doc["arr"][2].inline_comment == "# three"

    def test_array_inline_comment_clear(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][1].inline_comment = "# temp"
        assert doc["arr"][1].inline_comment == "# temp"
        doc["arr"][1].inline_comment = None
        assert doc["arr"][1].inline_comment is None
        assert "# temp" not in str(doc)

    def test_array_inline_comment_replace(self) -> None:
        toml = "arr = [\n  1, # old\n  2,\n]\n"
        doc = Document.parse(toml)
        assert doc["arr"][0].inline_comment == "# old"
        doc["arr"][0].inline_comment = "# new"
        assert doc["arr"][0].inline_comment == "# new"
        assert "# old" not in str(doc)
        assert "# new" in str(doc)

    def test_array_inline_comment_preserves_values(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        doc["arr"][0].inline_comment = "# note"
        assert doc["arr"][0] == 1
        assert doc["arr"][1] == 2

    def test_array_inline_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][0].inline_comment = "# wide indent"
        s = str(doc)
        assert "\n    2," in s  # 4-space indent preserved

    def test_array_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][1].comment = "# about two"
        s = str(doc)
        assert "    # about two\n    2" in s

    def test_array_comment_clear(self) -> None:
        toml = "arr = [\n  1,\n  # note\n  2,\n]\n"
        doc = Document.parse(toml)
        doc["arr"][1].comment = None
        assert "# note" not in str(doc)

    def test_array_inline_comment_on_single_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n]\n")
        doc["arr"][0].inline_comment = "# only"
        s = str(doc)
        assert "1, # only" in s
        doc2 = Document.parse(s)
        assert doc2["arr"][0].inline_comment == "# only"

    def test_array_comment_valid_toml(self) -> None:
        """Every array comment operation should produce re-parseable TOML."""
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][1].comment = "# block"
        doc["arr"][2].inline_comment = "# last"
        Document.parse(str(doc))  # should not raise

    def test_array_inline_and_block_coexist(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# inline on first"
        doc["arr"][1].comment = "# block before second"
        s = str(doc)
        assert "1, # inline on first" in s
        assert "  # block before second\n  2" in s
        assert doc["arr"][0].inline_comment == "# inline on first"
        assert doc["arr"][1].comment == "# block before second"
        Document.parse(s)  # valid TOML

    def test_array_inline_and_block_native_roundtrip(self) -> None:
        toml = "arr = [\n  1, # inline\n  # block\n  2,\n]\n"
        doc = Document.parse(toml)
        assert str(doc) == toml
        assert doc["arr"][0].inline_comment == "# inline"
        assert doc["arr"][1].comment == "# block"

    # ---- blank lines ----

    def test_blank_line_before_comment_roundtrip(self) -> None:
        doc = Document.parse("a = 1\n\n# note\nb = 2\n")
        assert doc["b"].comment == "\n# note"
        assert str(doc) == "a = 1\n\n# note\nb = 2\n"

    def test_set_blank_line_before_comment(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        doc["b"].comment = "\n# note"
        assert str(doc) == "a = 1\n\n# note\nb = 2\n"
        assert doc["b"].comment == "\n# note"

    def test_multiple_blank_lines_before_comment(self) -> None:
        doc = Document.parse("a = 1\n\n\n# note\nb = 2\n")
        assert doc["b"].comment == "\n\n# note"

    def test_blank_line_between_comment_lines(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        doc["b"].comment = "# first\n\n# second"
        s = str(doc)
        assert "# first\n\n# second\n" in s
        assert doc["b"].comment == "# first\n\n# second"

    # ---- validation (consolidated) ----

    def test_inline_comment_rejects_newlines(self) -> None:
        """Both scalar and array inline comments must reject newlines."""
        doc = Document.parse("key = 42\narr = [\n  1,\n  2,\n]\n")
        with pytest.raises(ValueError, match="newlines"):
            doc["key"].inline_comment = "# line1\nline2"
        with pytest.raises(ValueError, match="newlines"):
            doc["arr"][0].inline_comment = "# a\nb"

    def test_inline_comment_requires_hash(self) -> None:
        """Both scalar and array inline comments must start with #."""
        doc = Document.parse("key = 42\narr = [\n  1,\n  2,\n]\n")
        with pytest.raises(ValueError, match="#"):
            doc["key"].inline_comment = "no hash"
        with pytest.raises(ValueError, match="#"):
            doc["arr"][0].inline_comment = "no hash"

    def test_comment_requires_hash(self) -> None:
        """Both scalar and array block comments must start with #."""
        doc = Document.parse("key = 42\narr = [\n  1,\n  2,\n]\n")
        with pytest.raises(ValueError, match="#"):
            doc["key"].comment = "no hash"
        with pytest.raises(ValueError, match="#"):
            doc["arr"][1].comment = "no hash"

    def test_lone_hash_inline(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].inline_comment = "#"
        assert "42 #" in str(doc)
        assert doc["key"].inline_comment == "#"

    def test_lone_hash_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "#"
        s = str(doc)
        assert "#\nkey" in s
        assert doc["key"].comment == "#"

    def test_inline_comment_on_aot_rejected(self) -> None:
        doc = Document.parse("[[s]]\nname = 'a'\n[[s]]\nname = 'b'\n")
        with pytest.raises(TypeError, match="does not support"):
            doc["s"].inline_comment = "# nope"

    def test_comment_on_aot_element_rejected(self) -> None:
        doc = Document.parse("[[s]]\nname = 'a'\n[[s]]\nname = 'b'\n")
        with pytest.raises(TypeError, match="does not support"):
            doc["s"][0].comment = "# nope"
