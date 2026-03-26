"""Unit tests for atlassian.shared.formatters."""

import pytest
from atlassian.shared.formatters import strip_html, truncate, adf_to_text


class TestStripHtml:
    def test_removes_tags(self):
        assert strip_html("<p>Hello</p>") == "Hello"

    def test_br_becomes_newline(self):
        result = strip_html("line1<br/>line2")
        assert "line1" in result
        assert "line2" in result
        assert "\n" in result

    def test_block_elements_add_newline(self):
        result = strip_html("<p>Para 1</p><p>Para 2</p>")
        assert "Para 1" in result
        assert "Para 2" in result

    def test_decodes_html_entities(self):
        assert "&amp;" not in strip_html("a &amp; b")
        assert "&nbsp;" not in strip_html("a&nbsp;b")
        assert "&lt;" not in strip_html("a &lt; b")
        assert "&gt;" not in strip_html("a &gt; b")

    def test_collapses_excess_newlines(self):
        result = strip_html("<p>a</p><p>b</p><p>c</p>")
        assert "\n\n\n" not in result

    def test_strips_nested_tags(self):
        result = strip_html("<div><span><b>Bold</b></span></div>")
        assert result == "Bold"

    def test_empty_string(self):
        assert strip_html("") == ""

    def test_plain_text_unchanged(self):
        assert strip_html("hello world") == "hello world"

    def test_heading_tags(self):
        result = strip_html("<h1>Title</h1><p>Body</p>")
        assert "Title" in result
        assert "Body" in result


class TestTruncate:
    def test_short_text_unchanged(self):
        text = "hello"
        assert truncate(text, 100) == text

    def test_exact_length_unchanged(self):
        text = "a" * 100
        assert truncate(text, 100) == text

    def test_long_text_truncated(self):
        text = "a" * 200
        result = truncate(text, 100)
        assert len(result) > 100  # includes the truncation message
        assert "truncated" in result
        assert "200 chars" in result

    def test_truncation_preserves_start(self):
        text = "hello" + "x" * 200
        result = truncate(text, 10)
        assert result.startswith("hello")

    def test_default_max_len(self):
        text = "a" * 49999
        assert truncate(text) == text  # under default 50000

    def test_default_max_len_exceeded(self):
        text = "a" * 60000
        result = truncate(text)
        assert "truncated" in result


class TestAdfToText:
    def test_simple_paragraph(self):
        adf = {
            "type": "doc",
            "version": 1,
            "content": [
                {
                    "type": "paragraph",
                    "content": [{"type": "text", "text": "Hello world"}],
                }
            ],
        }
        result = adf_to_text(adf)
        assert "Hello world" in result

    def test_bullet_list(self):
        adf = {
            "type": "bulletList",
            "content": [
                {
                    "type": "listItem",
                    "content": [
                        {
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "Item 1"}],
                        }
                    ],
                },
                {
                    "type": "listItem",
                    "content": [
                        {
                            "type": "paragraph",
                            "content": [{"type": "text", "text": "Item 2"}],
                        }
                    ],
                },
            ],
        }
        result = adf_to_text(adf)
        assert "Item 1" in result
        assert "Item 2" in result

    def test_non_dict_returns_str(self):
        assert adf_to_text("plain text") == "plain text"

    def test_code_block(self):
        adf = {
            "type": "codeBlock",
            "content": [{"type": "text", "text": "print('hello')"}],
        }
        result = adf_to_text(adf)
        assert "print('hello')" in result
        assert "```" in result

    def test_empty_doc(self):
        adf = {"type": "doc", "version": 1, "content": []}
        result = adf_to_text(adf)
        assert result == ""
