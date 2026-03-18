"""Tests for the .comment (block) and .inline_comment (inline) properties."""

from __future__ import annotations

import pytest

from tests.conftest import toml_literal
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
        assert doc.as_toml() == "key = 42 # added\n"
        assert doc["key"].inline_comment == "# added"

    def test_replace_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # old\n")
        doc["key"].inline_comment = "# new"
        assert doc["key"].inline_comment == "# new"
        assert doc.as_toml() == "key = 42 # new\n"

    def test_clear_inline_comment(self) -> None:
        doc = Document.parse("key = 42 # remove me\n")
        doc["key"].inline_comment = None
        assert doc["key"].inline_comment is None
        assert doc.as_toml() == "key = 42\n"

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
        assert doc.as_toml() == "# added\nkey = 42\n"
        assert doc["key"].comment == "# added"

    def test_replace_comment(self) -> None:
        doc = Document.parse("# old\nkey = 42\n")
        doc["key"].comment = "# new"
        assert doc["key"].comment == "# new"
        assert doc.as_toml() == "# new\nkey = 42\n"

    def test_clear_comment(self) -> None:
        doc = Document.parse("# remove\nkey = 42\n")
        doc["key"].comment = None
        assert doc["key"].comment is None
        assert doc.as_toml() == "key = 42\n"

    def test_set_multiline_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# line A\n# line B"
        assert doc.as_toml() == toml_literal("""
            # line A
            # line B
            key = 42
        """)
        assert doc["key"].comment == "# line A\n# line B"

    # ---- both together ----

    def test_both_comments(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "# above"
        doc["key"].inline_comment = "# inline"
        assert doc.as_toml() == "# above\nkey = 42 # inline\n"
        assert doc["key"].comment == "# above"
        assert doc["key"].inline_comment == "# inline"

    # ---- round-trip ----

    def test_comment_roundtrip(self) -> None:
        toml = "# important\nkey = 42 # magic number\n"
        doc = Document.parse(toml)
        assert doc.as_toml() == toml

    # ---- array element comments ----

    def test_set_inline_comment_on_array_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][2].inline_comment = "# last"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # first
              2,
              3, # last
            ]
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["arr"][0].inline_comment == "# first"
        assert doc2["arr"][1].inline_comment is None
        assert doc2["arr"][2].inline_comment == "# last"

    def test_array_inline_comment_roundtrip(self) -> None:
        toml = 'arr = [\n  "a", # one\n  "b", # two\n  "c", # three\n]\n'
        doc = Document.parse(toml)
        assert doc.as_toml() == toml
        assert doc["arr"][0].inline_comment == "# one"
        assert doc["arr"][1].inline_comment == "# two"
        assert doc["arr"][2].inline_comment == "# three"

    def test_array_inline_comment_clear(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][1].inline_comment = "# temp"
        assert doc["arr"][1].inline_comment == "# temp"
        doc["arr"][1].inline_comment = None
        assert doc["arr"][1].inline_comment is None
        assert doc.as_toml() == toml_literal("""
            arr = [
              1,
              2,
              3,
            ]
        """)

    def test_replace_array_inline_comment(self) -> None:
        toml = "arr = [\n  1, # old\n  2,\n]\n"
        doc = Document.parse(toml)
        assert doc["arr"][0].inline_comment == "# old"
        doc["arr"][0].inline_comment = "# new"
        assert doc["arr"][0].inline_comment == "# new"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # new
              2,
            ]
        """)

    def test_array_inline_comment_preserves_values(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n]\n")
        doc["arr"][0].inline_comment = "# note"
        assert doc["arr"][0] == 1
        assert doc["arr"][1] == 2

    def test_array_inline_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][0].inline_comment = "# wide indent"
        assert doc.as_toml() == toml_literal("""
            arr = [
                1, # wide indent
                2,
            ]
        """)

    def test_array_comment_preserves_indentation(self) -> None:
        doc = Document.parse("arr = [\n    1,\n    2,\n]\n")
        doc["arr"][1].comment = "# about two"
        assert doc.as_toml() == toml_literal("""
            arr = [
                1,
                # about two
                2,
            ]
        """)

    def test_array_comment_clear(self) -> None:
        toml = "arr = [\n  1,\n  # note\n  2,\n]\n"
        doc = Document.parse(toml)
        doc["arr"][1].comment = None
        assert doc.as_toml() == toml_literal("""
            arr = [
              1,
              2,
            ]
        """)

    def test_array_inline_comment_on_single_element(self) -> None:
        doc = Document.parse("arr = [\n  1,\n]\n")
        doc["arr"][0].inline_comment = "# only"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # only
            ]
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["arr"][0].inline_comment == "# only"

    def test_inline_comment_stable_after_append(self) -> None:
        doc = Document()
        doc["array"] = ["zero", "one"]
        doc["array"][1].inline_comment = "# better than zero"
        doc["array"].append("two")
        assert doc["array"][1].inline_comment == "# better than zero"
        assert doc["array"][2].inline_comment is None

    def test_inline_comment_stable_after_append_multiline(self) -> None:
        doc = Document.parse('arr = [\n  "a",\n  "b", # note\n]\n')
        assert doc["arr"][1].inline_comment == "# note"
        doc["arr"].append("c")
        assert doc["arr"][1].inline_comment == "# note"
        assert doc["arr"][2].inline_comment is None

    def test_inline_comment_stable_after_extend(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n]\n")
        doc["arr"].extend([2, 3])
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None
        assert doc["arr"][2].inline_comment is None

    def test_inline_comment_stable_after_iadd(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n]\n")
        doc["arr"] += [2, 3]
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None

    # ---- inline comments transfer across arrays ----

    def test_extend_transfers_inline_comment(self) -> None:
        doc = Document()
        doc["dst"] = ["a"]
        doc["src"] = ["b", "c"]
        doc["src"][0].inline_comment = "# from b"
        doc["src"][1].inline_comment = "# from c"
        doc["dst"].extend(doc["src"])
        assert doc["dst"][1].inline_comment == "# from b"
        assert doc["dst"][2].inline_comment == "# from c"

    def test_append_transfers_inline_comment(self) -> None:
        doc = Document.parse("src = [\n  1, # noted\n]\ndst = [0]\n")
        doc["dst"].append(doc["src"][0])
        assert doc["dst"][1].inline_comment == "# noted"

    def test_insert_transfers_inline_comment(self) -> None:
        doc = Document.parse("src = [\n  1, # noted\n]\ndst = [0, 2]\n")
        doc["dst"].insert(1, doc["src"][0])
        assert doc["dst"][0].inline_comment is None
        assert doc["dst"][1].inline_comment == "# noted"
        assert doc["dst"][2].inline_comment is None

    def test_setitem_transfers_inline_comment(self) -> None:
        doc = Document.parse("src = [\n  1, # noted\n]\ndst = [0, 2]\n")
        doc["dst"][0] = doc["src"][0]
        assert doc["dst"][0].inline_comment == "# noted"
        assert doc["dst"][1].inline_comment is None

    def test_setitem_preserves_existing_when_source_has_none(self) -> None:
        doc = Document.parse("arr = [\n  1, # keep\n]\nplain = [99]\n")
        doc["arr"][0] = doc["plain"][0]
        assert doc["arr"][0].inline_comment == "# keep"

    def test_extended_slice_transfers_inline_comment(self) -> None:
        doc = Document.parse("src = [\n  1, # s1\n  2, # s2\n]\ndst = [0, 0, 0]\n")
        doc["dst"][::2] = [doc["src"][0], doc["src"][1]]
        assert doc["dst"][0].inline_comment == "# s1"
        assert doc["dst"][1].inline_comment is None
        assert doc["dst"][2].inline_comment == "# s2"

    def test_iadd_transfers_inline_comment(self) -> None:
        doc = Document()
        doc["dst"] = [0]
        doc["src"] = [1]
        doc["src"][0].inline_comment = "# tag"
        doc["dst"] += doc["src"]
        assert doc["dst"][1].inline_comment == "# tag"

    def test_scalar_inline_comment_transfers_to_array(self) -> None:
        doc = Document()
        doc["val"] = 42
        doc["val"].inline_comment = "# answer"
        doc["arr"] = [1]
        doc["arr"].append(doc["val"])
        assert doc["arr"][1].inline_comment == "# answer"

    # ---- insert preserves comments ----

    def test_insert_middle_preserves_prev_comment(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n  2, # second\n  3, # third\n]\n")
        doc["arr"].insert(1, 99)
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None
        assert doc["arr"][2].inline_comment == "# second"
        assert doc["arr"][3].inline_comment == "# third"

    def test_insert_at_start_no_comment_shift(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n  2, # second\n]\n")
        doc["arr"].insert(0, 99)
        assert doc["arr"][0].inline_comment is None
        assert doc["arr"][1].inline_comment == "# first"
        assert doc["arr"][2].inline_comment == "# second"

    def test_insert_at_end_preserves_prev_comment(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n  2, # second\n]\n")
        doc["arr"].insert(2, 99)
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment == "# second"
        assert doc["arr"][2].inline_comment is None

    # ---- removal preserves comments ----

    def test_pop_last_drops_its_comment(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3, # last\n]\n")
        doc["arr"].pop()
        assert doc["arr"][0].inline_comment is None
        assert doc["arr"][1].inline_comment is None

    def test_pop_last_preserves_prev_comment(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2, # second\n  3,\n]\n")
        doc["arr"].pop()
        assert doc["arr"][1].inline_comment == "# second"

    def test_remove_middle_preserves_prev_comment(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n  2,\n  3,\n]\n")
        doc["arr"].remove(2)
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment is None

    def test_del_middle_all_commented(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n  2, # second\n  3, # third\n]\n")
        del doc["arr"][1]
        assert doc["arr"][0].inline_comment == "# first"
        assert doc["arr"][1].inline_comment == "# third"

    def test_pop_first_drops_its_comment(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n  2, # second\n  3, # third\n]\n")
        doc["arr"].pop(0)
        assert doc["arr"][0].inline_comment == "# second"
        assert doc["arr"][1].inline_comment == "# third"

    def test_slice_del_preserves_survivor_comments(self) -> None:
        doc = Document.parse("arr = [\n  1, # first\n  2, # second\n  3, # third\n]\n")
        del doc["arr"][0:2]
        assert doc["arr"][0].inline_comment == "# third"

    def test_array_comment_valid_toml(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# first"
        doc["arr"][1].comment = "# block"
        doc["arr"][2].inline_comment = "# last"
        Document.parse(doc.as_toml())  # should not raise

    def test_array_inline_and_block_coexist(self) -> None:
        doc = Document.parse("arr = [\n  1,\n  2,\n  3,\n]\n")
        doc["arr"][0].inline_comment = "# inline on first"
        doc["arr"][1].comment = "# block before second"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1, # inline on first
              # block before second
              2,
              3,
            ]
        """)
        assert doc["arr"][0].inline_comment == "# inline on first"
        assert doc["arr"][1].comment == "# block before second"
        Document.parse(doc.as_toml())  # valid TOML

    def test_array_inline_and_block_native_roundtrip(self) -> None:
        toml = "arr = [\n  1, # inline\n  # block\n  2,\n]\n"
        doc = Document.parse(toml)
        assert doc.as_toml() == toml
        assert doc["arr"][0].inline_comment == "# inline"
        assert doc["arr"][1].comment == "# block"

    # ---- blank lines ----

    def test_blank_line_before_comment_roundtrip(self) -> None:
        doc = Document.parse("a = 1\n\n# note\nb = 2\n")
        assert doc["b"].comment == "\n# note"
        assert doc.as_toml() == toml_literal("""
            a = 1

            # note
            b = 2
        """)

    def test_set_blank_line_before_comment(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        doc["b"].comment = "\n# note"
        assert doc.as_toml() == toml_literal("""
            a = 1

            # note
            b = 2
        """)
        assert doc["b"].comment == "\n# note"

    def test_multiple_blank_lines_before_comment(self) -> None:
        doc = Document.parse("a = 1\n\n\n# note\nb = 2\n")
        assert doc["b"].comment == "\n\n# note"

    def test_blank_line_between_comment_lines(self) -> None:
        doc = Document.parse("a = 1\nb = 2\n")
        doc["b"].comment = "# first\n\n# second"
        assert doc.as_toml() == toml_literal("""
            a = 1
            # first

            # second
            b = 2
        """)
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
        assert doc.as_toml() == "key = 42 #\n"
        assert doc["key"].inline_comment == "#"

    def test_lone_hash_comment(self) -> None:
        doc = Document.parse("key = 42\n")
        doc["key"].comment = "#"
        assert doc.as_toml() == "#\nkey = 42\n"
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
        assert doc.as_toml() == toml_literal("""
            # added
            [section]
            x = 1
        """)
        assert doc["section"].comment == "# added"

    def test_set_table_comment_from_scratch(self) -> None:
        doc = Document()
        doc["foo"] = {"this": "that"}
        doc["foo"].comment = "# Hello"
        assert doc.as_toml() == toml_literal("""
            # Hello
            [foo]
            this = "that"
        """)
        doc2 = Document.parse(doc.as_toml())
        assert doc2["foo"]["this"] == "that"
        assert doc2["foo"].comment == "# Hello"

    def test_replace_table_comment(self) -> None:
        doc = Document.parse("# old\n[section]\nx = 1\n")
        doc["section"].comment = "# new"
        assert doc.as_toml() == toml_literal("""
            # new
            [section]
            x = 1
        """)
        assert doc["section"].comment == "# new"

    def test_clear_table_comment(self) -> None:
        doc = Document.parse("# remove\n[section]\nx = 1\n")
        doc["section"].comment = None
        assert doc.as_toml() == "\n[section]\nx = 1\n"
        assert doc["section"].comment is None

    def test_multiline_table_comment(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"].comment = "# line A\n# line B"
        assert doc.as_toml() == toml_literal("""
            # line A
            # line B
            [section]
            x = 1
        """)
        assert doc["section"].comment == "# line A\n# line B"

    def test_table_comment_roundtrip(self) -> None:
        toml = "# important\n[section]\nkey = 42\n"
        doc = Document.parse(toml)
        assert doc["section"].comment == "# important"
        assert doc.as_toml() == toml

    def test_table_both_comments(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"].comment = "# above"
        doc["section"].inline_comment = "# inline"
        assert doc.as_toml() == toml_literal("""
            # above
            [section] # inline
            x = 1
        """)
        assert doc["section"].comment == "# above"
        assert doc["section"].inline_comment == "# inline"

    # ---- table section inline comments ----

    def test_read_table_section_inline_comment(self) -> None:
        doc = Document.parse("[section] # my note\nx = 1\n")
        assert doc["section"].inline_comment == "# my note"

    def test_set_table_section_inline_comment(self) -> None:
        doc = Document.parse("[section]\nx = 1\n")
        doc["section"].inline_comment = "# added"
        assert doc.as_toml() == "[section] # added\nx = 1\n"
        assert doc["section"].inline_comment == "# added"

    def test_clear_table_section_inline_comment(self) -> None:
        doc = Document.parse("[section] # comment\nx = 1\n")
        doc["section"].inline_comment = None
        assert doc["section"].inline_comment is None
        assert doc.as_toml() == "[section]\nx = 1\n"

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
        assert doc.as_toml() == "t = {# note\nx = 1, y = 2}\n"
        doc2 = Document.parse(doc.as_toml())
        assert doc2["t"]["x"] == 1
        assert doc2["t"]["y"] == 2
        assert doc2["t"]["x"].comment == "# note"

    def test_clear_inline_table_key_comment(self) -> None:
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].comment = None
        assert doc["t"]["x"].comment is None
        assert doc.as_toml() == "t = {x = 1, y = 2}\n"

    def test_set_inline_comment_on_inline_table_value(self) -> None:
        """Setting inline comment on inline table value produces multiline."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].inline_comment = "# boom"
        result = doc.as_toml()
        assert "# boom" in result
        doc2 = Document.parse(result)
        assert doc2["t"]["x"] == 1
        assert doc2["t"]["y"] == 2
        assert doc2["t"]["x"].inline_comment == "# boom"

    def test_read_inline_comment_on_inline_table_value(self) -> None:
        """Inline comments on inline table values can be read."""
        doc = Document.parse("t = {\n  x = 1, # note on x\n  y = 2,\n}\n")
        assert doc["t"]["x"].inline_comment == "# note on x"
        assert doc["t"]["y"].inline_comment is None

    def test_read_inline_comment_on_last_inline_table_value(self) -> None:
        """Last element's inline comment lives in trailing."""
        doc = Document.parse("t = {\n  x = 1,\n  y = 2, # note on y\n}\n")
        assert doc["t"]["x"].inline_comment is None
        assert doc["t"]["y"].inline_comment == "# note on y"

    def test_read_inline_comment_all_values(self) -> None:
        """All inline table values can have inline comments."""
        src = "t = {\n  a = 1, # ca\n  b = 2, # cb\n  c = 3, # cc\n}\n"
        doc = Document.parse(src)
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["b"].inline_comment == "# cb"
        assert doc["t"]["c"].inline_comment == "# cc"

    def test_set_and_clear_inline_comment_on_inline_table_value(self) -> None:
        """Set then clear inline comment."""
        doc = Document.parse("t = {\n  x = 1,\n  y = 2,\n}\n")
        doc["t"]["x"].inline_comment = "# hi"
        assert doc["t"]["x"].inline_comment == "# hi"
        doc["t"]["x"].inline_comment = None
        assert doc["t"]["x"].inline_comment is None

    def test_clear_inline_comment_on_inline_table_value_allowed(self) -> None:
        """Clearing (None) is always safe, even inside inline tables."""
        doc = Document.parse("t = {x = 1, y = 2}\n")
        doc["t"]["x"].inline_comment = None
        assert doc.as_toml() == "t = {x = 1, y = 2}\n"

    def test_inline_comment_single_key_inline_table(self) -> None:
        """Single-key inline table: comment goes to trailing."""
        doc = Document.parse("t = {\n  x = 1,\n}\n")
        doc["t"]["x"].inline_comment = "# only"
        assert doc["t"]["x"].inline_comment == "# only"
        result = doc.as_toml()
        doc2 = Document.parse(result)
        assert doc2["t"]["x"].inline_comment == "# only"

    def test_inline_comment_coexists_with_block_comment(self) -> None:
        """Inline comment on x and block comment on y are independent."""
        src = "t = {\n  x = 1, # inline on x\n  # block on y\n  y = 2,\n}\n"
        doc = Document.parse(src)
        assert doc["t"]["x"].inline_comment == "# inline on x"
        assert doc["t"]["y"].comment == "# block on y"
        assert doc["t"]["y"].inline_comment is None
        assert doc["t"]["x"].comment is None

    def test_delete_key_preserves_sibling_inline_comments(self) -> None:
        """Deleting middle key preserves other keys' inline comments."""
        src = "t = {\n  a = 1, # ca\n  b = 2, # cb\n  c = 3, # cc\n}\n"
        doc = Document.parse(src)
        del doc["t"]["b"]
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["c"].inline_comment == "# cc"

    def test_delete_first_key_preserves_inline_comments(self) -> None:
        """Deleting first key preserves remaining comments."""
        src = "t = {\n  a = 1, # ca\n  b = 2, # cb\n}\n"
        doc = Document.parse(src)
        del doc["t"]["a"]
        assert doc["t"]["b"].inline_comment == "# cb"

    def test_delete_last_key_preserves_inline_comments(self) -> None:
        """Deleting last key preserves remaining comments."""
        src = "t = {\n  a = 1, # ca\n  b = 2, # cb\n}\n"
        doc = Document.parse(src)
        del doc["t"]["b"]
        assert doc["t"]["a"].inline_comment == "# ca"

    def test_pop_key_preserves_inline_comments(self) -> None:
        """pop() preserves sibling inline comments."""
        src = "t = {\n  a = 1, # ca\n  b = 2, # cb\n  c = 3, # cc\n}\n"
        doc = Document.parse(src)
        val = doc["t"].pop("b")
        assert val == 2
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["c"].inline_comment == "# cc"

    def test_add_key_preserves_inline_comments(self) -> None:
        """Adding a new key preserves existing inline comments."""
        src = "t = {\n  a = 1, # ca\n  b = 2, # cb\n}\n"
        doc = Document.parse(src)
        doc["t"]["c"] = 3
        assert doc["t"]["a"].inline_comment == "# ca"
        assert doc["t"]["b"].inline_comment == "# cb"
        assert doc["t"]["c"].inline_comment is None

    def test_update_preserves_inline_comments(self) -> None:
        """update() with new keys preserves existing inline comments."""
        src = "t = {\n  a = 1, # ca\n}\n"
        doc = Document.parse(src)
        doc["t"].update({"b": 2})
        assert doc["t"]["a"].inline_comment == "# ca"

    def test_setdefault_preserves_inline_comments(self) -> None:
        """setdefault() with new key preserves existing inline comments."""
        src = "t = {\n  a = 1, # ca\n}\n"
        doc = Document.parse(src)
        doc["t"].setdefault("b", 2)
        assert doc["t"]["a"].inline_comment == "# ca"

    def test_clone_inline_table_value_carries_inline_comment(self) -> None:
        """Assigning a value from an inline table copies its inline comment."""
        src = "t = {\n  x = 1, # travel\n  y = 2,\n}\narr = [0]\n"
        doc = Document.parse(src)
        doc["arr"][0] = doc["t"]["x"]
        assert doc["arr"][0].inline_comment == "# travel"

    def test_inline_table_comment_not_misattributed(self) -> None:
        """An inline comment after x must not appear as y's block comment."""
        doc = Document.parse("t = {\n  x = 1, # note on x\n  y = 2,\n}\n")
        assert doc["t"]["x"].comment is None
        assert doc["t"]["y"].comment is None

    def test_inline_table_set_comment_preserves_prev_inline(self) -> None:
        """Setting y's block comment must not clobber x's inline comment."""
        src = "t = {\n  x = 1, # note on x\n  y = 2,\n}\n"
        doc = Document.parse(src)
        doc["t"]["y"].comment = "# block on y"
        result = doc.as_toml()
        # x's inline comment survives in the serialised output
        assert "# note on x" in result
        assert "# block on y" in result
        # roundtrip: y's block comment reads back correctly
        doc2 = Document.parse(result)
        assert doc2["t"]["y"].comment == "# block on y"

    def test_inline_table_clear_comment_preserves_prev_inline(self) -> None:
        """Clearing y's block comment preserves x's inline comment."""
        src = "t = {\n  x = 1, # note on x\n  # block on y\n  y = 2,\n}\n"
        doc = Document.parse(src)
        assert doc["t"]["y"].comment == "# block on y"
        doc["t"]["y"].comment = None
        result = doc.as_toml()
        assert "# note on x" in result
        doc2 = Document.parse(result)
        assert doc2["t"]["y"].comment is None

    def test_inline_table_both_inline_and_block_separated(self) -> None:
        """When the prefix has both an inline comment from the previous
        element and a block comment for this key, only the block portion
        is returned by .comment."""
        src = "t = {\n  x = 1, # inline on x\n  # block on y\n  y = 2,\n}\n"
        doc = Document.parse(src)
        assert doc["t"]["x"].comment is None
        assert doc["t"]["y"].comment == "# block on y"

    # ---- array comment edge cases ----

    def test_multiline_array_with_blank_line_between_comments(self) -> None:
        """Tests split_prefix where block has multiple newlines."""
        toml = "arr = [\n  # first\n  1,\n\n  # after blank\n  2,\n]\n"
        doc = Document.parse(toml)
        # blank line is preserved as part of the block comment
        assert doc["arr"][1].comment == "\n# after blank"

    def test_set_comment_on_multiline_array_element(self) -> None:
        """Set a block comment on an element in a multiline array."""
        toml = "arr = [\n  1,\n  2,\n  3,\n]\n"
        doc = Document.parse(toml)
        doc["arr"][1].comment = "# middle"
        assert doc.as_toml() == toml_literal("""
            arr = [
              1,
              # middle
              2,
              3,
            ]
        """)
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
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a" # first
            [[items]]
            name = "b"
        """)

    def test_set_comment_on_aot_entry_child(self) -> None:
        doc = Document.parse('[[items]]\nname = "a"\n[[items]]\nname = "b"\n')
        doc["items"][1]["name"].comment = "# second item name"
        assert doc["items"][1]["name"].comment == "# second item name"
        assert doc.as_toml() == toml_literal("""
            [[items]]
            name = "a"
            [[items]]
            # second item name
            name = "b"
        """)

    # ---- comment edge cases ----

    def test_inline_comment_on_aot_itself_is_none(self) -> None:
        """AoT item has no decor suffix — inline_comment returns None."""
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert doc["items"].inline_comment is None

    def test_comment_on_aot_itself_is_none(self) -> None:
        """AoT has no key prefix — comment returns None."""
        doc = Document.parse('[[items]]\nname = "a"\n')
        assert doc["items"].comment is None

    # ---- comments survive mutations ----

    def test_inline_comment_preserved_on_top_level_update(self) -> None:
        toml = 'title = "old" # important note\n'
        doc = Document.parse(toml)
        doc["title"] = "new"
        assert doc.as_toml() == 'title = "new" # important note\n'

    def test_inline_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\nname = "Tom"  # the owner name\nage = 30\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        assert doc.as_toml() == toml_literal("""
            [owner]
            name = "Bob"  # the owner name
            age = 30
        """)

    def test_standalone_comment_preserved_on_nested_update(self) -> None:
        toml = '[owner]\n# this is the name\nname = "Tom"\n'
        doc = Document.parse(toml)
        doc["owner"]["name"] = "Bob"
        assert doc.as_toml() == toml_literal("""
            [owner]
            # this is the name
            name = "Bob"
        """)
