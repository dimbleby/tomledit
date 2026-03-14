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
        assert str(doc) == "key = 42 # added\n"
        assert doc["key"].inline_comment == "# added"

    def test_replace_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # old\n")
        doc["key"].inline_comment = "# new"
        assert doc["key"].inline_comment == "# new"
        assert str(doc) == "key = 42 # new\n"

    def test_clear_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # remove me\n")
        doc["key"].inline_comment = None
        assert doc["key"].inline_comment is None
        assert str(doc) == "key = 42\n"

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
        assert str(doc) == "# added\nkey = 42\n"
        assert doc["key"].comment == "# added"

    def test_replace_comment(self) -> None:
        doc = Document.parse("# old\nkey = 42\n")
        doc["key"].comment = "# new"
        assert doc["key"].comment == "# new"
        assert str(doc) == "# new\nkey = 42\n"

    def test_clear_comment(self) -> None:
        doc = Document.parse("# remove\nkey = 42\n")
        doc["key"].comment = None
        assert doc["key"].comment is None
        assert str(doc) == "key = 42\n"

    def test_multiline_comment_set(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# line A\n# line B"
        assert str(doc) == "# line A\n# line B\nkey = 42\n"
        assert doc["key"].comment == "# line A\n# line B"

    # ---- both together ----

    def test_both_comments(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# above"
        doc["key"].inline_comment = "# inline"
        assert str(doc) == "# above\nkey = 42 # inline\n"
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
        assert str(doc) == "arr = [\n  1,\n  # about two\n  2,\n]\n"

    def test_set_inline_comment_on_array_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][2].inline_comment = "# last"
        assert str(doc) == "arr = [\n  1, # first\n  2,\n  3, # last\n]\n"
        doc2 = Document.parse(str(doc))
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
        assert str(doc) == "arr = [\n  1,\n  2,\n  3,\n]\n"

    def test_array_inline_comment_replace(self) -> None:
        toml = "arr = [\n  1, # old\n  2,\n]\n"
        doc = Document.parse(toml)
        assert doc["arr"][0].inline_comment == "# old"
        doc["arr"][0].inline_comment = "# new"
        assert doc["arr"][0].inline_comment == "# new"
        assert str(doc) == "arr = [\n  1, # new\n  2,\n]\n"

    def test_array_inline_comment_preserves_values(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        doc["arr"][0].inline_comment = "# note"
        assert doc["arr"][0] == 1
        assert doc["arr"][1] == 2

    def test_array_inline_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][0].inline_comment = "# wide indent"
        assert str(doc) == "arr = [\n    1, # wide indent\n    2,\n]\n"

    def test_array_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][1].comment = "# about two"
        assert str(doc) == "arr = [\n    1,\n    # about two\n    2,\n]\n"

    def test_array_comment_clear(self) -> None:
        toml = "arr = [\n  1,\n  # note\n  2,\n]\n"
        doc = Document.parse(toml)
        doc["arr"][1].comment = None
        assert str(doc) == "arr = [\n  1,\n  2,\n]\n"

    def test_array_inline_comment_on_single_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n]\n")
        doc["arr"][0].inline_comment = "# only"
        assert str(doc) == "arr = [\n  1, # only\n]\n"
        doc2 = Document.parse(str(doc))
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
        assert str(doc) == (
            "arr = [\n  1, # inline on first\n  # block before second\n  2,\n  3,\n]\n"
        )
        assert doc["arr"][0].inline_comment == "# inline on first"
        assert doc["arr"][1].comment == "# block before second"
        Document.parse(str(doc))  # valid TOML

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
        assert str(doc) == "a = 1\n# first\n\n# second\nb = 2\n"
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
        assert str(doc) == "key = 42 #\n"
        assert doc["key"].inline_comment == "#"

    def test_lone_hash_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "#"
        assert str(doc) == "#\nkey = 42\n"
        assert doc["key"].comment == "#"

    def test_inline_comment_on_aot_rejected(self) -> None:
        doc = Document.parse("[[s]]\nname = 'a'\n[[s]]\nname = 'b'\n")
        with pytest.raises(TypeError, match="does not support"):
            doc["s"].inline_comment = "# nope"

    def test_comment_on_aot_element_rejected(self) -> None:
        doc = Document.parse("[[s]]\nname = 'a'\n[[s]]\nname = 'b'\n")
        with pytest.raises(TypeError, match="does not support"):
            doc["s"][0].comment = "# nope"

    # ---- table section block comments ----

    def test_read_table_comment(self) -> None:
        doc = Document.parse("# before table\n[section]\nx = 1\n")
        assert doc["section"].comment == "# before table"

    def test_read_table_no_comment(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        assert doc["section"].comment is None

    def test_set_table_comment(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"].comment = "# added"
        assert str(doc) == "# added\n[section]\nx = 1\n"
        assert doc["section"].comment == "# added"

    def test_set_table_comment_from_scratch(self) -> None:
        doc = Document()
        doc["foo"] = {"this": "that"}
        doc["foo"].comment = "# Hello"
        assert str(doc) == '# Hello\n[foo]\nthis = "that"\n'
        doc2 = Document.parse(str(doc))
        assert doc2["foo"]["this"] == "that"
        assert doc2["foo"].comment == "# Hello"

    def test_replace_table_comment(self) -> None:
        doc = Document.parse("# old\n[section]\nx = 1\n")
        doc["section"].comment = "# new"
        assert str(doc) == "# new\n[section]\nx = 1\n"
        assert doc["section"].comment == "# new"

    def test_clear_table_comment(self) -> None:
        doc = Document.parse("# remove\n[section]\nx = 1\n")
        doc["section"].comment = None
        assert str(doc) == "\n[section]\nx = 1\n"
        assert doc["section"].comment is None

    def test_multiline_table_comment(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"].comment = "# line A\n# line B"
        assert str(doc) == "# line A\n# line B\n[section]\nx = 1\n"
        assert doc["section"].comment == "# line A\n# line B"

    def test_table_comment_roundtrip(self) -> None:
        toml = "# important\n[section]\nkey = 42\n"
        doc = Document.parse(toml)
        assert doc["section"].comment == "# important"
        assert str(doc) == toml

    def test_table_both_comments(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"].comment = "# above"
        doc["section"].inline_comment = "# inline"
        assert str(doc) == "# above\n[section] # inline\nx = 1\n"
        assert doc["section"].comment == "# above"
        assert doc["section"].inline_comment == "# inline"

    # ---- table section inline comments ----

    def test_read_table_section_inline_comment(self) -> None:
        doc = Document.parse("[section] # my note\nx = 1\n")
        assert doc["section"].inline_comment == "# my note"

    def test_set_table_section_inline_comment(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"].inline_comment = "# added"
        assert str(doc) == "[section] # added\nx = 1\n"
        assert doc["section"].inline_comment == "# added"

    def test_clear_table_section_inline_comment(self) -> None:
        doc = Document.parse("[section] # comment\nx = 1\n")
        doc["section"].inline_comment = None
        assert doc["section"].inline_comment is None
        assert str(doc) == "[section]\nx = 1\n"

    def test_table_without_inline_comment_returns_none(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        assert doc["section"].inline_comment is None

    # ---- inline table key comments ----

    def test_read_inline_table_key_comment_none(self) -> None:
        doc = Document.parse("t = {x = 1, y = 2}\n")
        assert doc["t"]["x"].comment is None

    def test_set_inline_table_key_comment(self) -> None:
        """Setting a block comment on an inline table key produces a
        multi-line inline table (valid TOML 1.1)."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].comment = "# note"
        assert str(doc) == "t = {# note\nx = 1, y = 2}\n"
        doc2 = Document.parse(str(doc))
        assert doc2["t"]["x"] == 1
        assert doc2["t"]["y"] == 2
        assert doc2["t"]["x"].comment == "# note"

    def test_clear_inline_table_key_comment(self) -> None:
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].comment = None
        assert doc["t"]["x"].comment is None
        assert str(doc) == "t = {x = 1, y = 2}\n"

    def test_inline_comment_on_inline_table_value_rejected(self) -> None:
        """Inline comments inside inline tables would produce invalid TOML
        because # comments out everything to end-of-line, eating `,` and `}`."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        with pytest.raises(TypeError, match="inline comment"):
            doc["t"]["x"].inline_comment = "# boom"
        with pytest.raises(TypeError, match="inline comment"):
            doc["t"]["y"].inline_comment = "# boom"

    def test_clear_inline_comment_on_inline_table_value_allowed(self) -> None:
        """Clearing (None) is always safe, even inside inline tables."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].inline_comment = None
        assert str(doc) == "t = {x = 1, y = 2}\n"

    # ---- array comment edge cases ----

    def test_multiline_array_with_blank_line_between_comments(self) -> None:
        """Tests split_prefix where block has multiple newlines."""
        toml = "arr = [\n  # first\n  1,\n\n  # after blank\n  2,\n]\n"
        doc = Document.parse(toml)
        # blank line is preserved as part of the block comment
        assert doc["arr"][1].comment == "\n# after blank"

    def test_array_element_comment_roundtrip_multiline(self) -> None:
        """Set a block comment on an element in a multiline array."""
        toml = "arr = [\n  1,\n  2,\n  3,\n]\n"
        doc = Document.parse(toml)
        doc["arr"][1].comment = "# middle"
        assert str(doc) == "arr = [\n  1,\n  # middle\n  2,\n  3,\n]\n"
        assert doc["arr"][1].comment == "# middle"

    def test_blank_line_between_elements_no_comment(self) -> None:
        """Blank line between elements with no comment - exercises
        split_prefix block.is_empty() branch."""
        doc = Document.parse("arr = [\n  1,\n\n  2,\n]\n")
        assert doc["arr"][0].comment is None
        assert doc["arr"][0].inline_comment is None

    def test_compact_array_comment_returns_none(self) -> None:
        """Compact single-line array - prefix has no newline at all,
        exercises split_prefix no-newline fallback."""
        doc = Document.parse("arr = [1, 2, 3]\n")
        assert doc["arr"][0].comment is None
        assert doc["arr"][1].comment is None
        assert doc["arr"][0].inline_comment is None

    # ---- comments on AoT children ----

    def test_set_inline_comment_on_aot_entry_child(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        doc["items"][0]["name"].inline_comment = "# first"
        assert doc["items"][0]["name"].inline_comment == "# first"
        assert str(doc) == '[[items]]\nname = "a" # first\n[[items]]\nname = "b"\n'

    def test_set_comment_on_aot_entry_child(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        doc["items"][1]["name"].comment = "# second item name"
        assert doc["items"][1]["name"].comment == "# second item name"
        assert (
            str(doc)
            == '[[items]]\nname = "a"\n[[items]]\n# second item name\nname = "b"\n'
        )

    # ---- comment edge cases ----

    def test_inline_comment_on_aot_itself_is_none(self) -> None:
        """AoT item has no decor suffix — inline_comment returns None."""
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert doc["items"].inline_comment is None

    def test_comment_on_aot_itself_is_none(self) -> None:
        """AoT has no key prefix — comment returns None."""
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert doc["items"].comment is None
