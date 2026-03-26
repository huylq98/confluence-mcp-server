"""Unit tests for atlassian.shared.url_utils."""

import pytest
from atlassian.shared.url_utils import parse_confluence_url


class TestParseConfluenceUrl:
    def test_page_id_query_param(self):
        result = parse_confluence_url("http://wiki.example.com/pages/viewpage.action?pageId=12345")
        assert result["page_id"] == "12345"
        assert result["space_key"] is None
        assert result["title"] is None

    def test_display_format(self):
        result = parse_confluence_url("http://wiki.example.com/display/DEV/My+Page+Title")
        assert result["space_key"] == "DEV"
        assert result["title"] == "My Page Title"
        assert result["page_id"] is None

    def test_display_format_with_query(self):
        result = parse_confluence_url(
            "http://wiki.example.com/display/DEV/Page?src=contextnavpagetreemode"
        )
        assert result["space_key"] == "DEV"
        assert result["title"] == "Page"

    def test_spaces_format(self):
        result = parse_confluence_url(
            "http://wiki.example.com/spaces/DEV/pages/99999/Some+Title"
        )
        assert result["space_key"] == "DEV"
        assert result["page_id"] == "99999"
        assert result["title"] == "Some Title"

    def test_wiki_prefix_stripped(self):
        result = parse_confluence_url(
            "http://mycompany.atlassian.net/wiki/display/ENG/Architecture"
        )
        assert result["space_key"] == "ENG"
        assert result["title"] == "Architecture"

    def test_confluence_prefix_stripped(self):
        result = parse_confluence_url(
            "http://wiki.example.com/confluence/display/HR/Onboarding"
        )
        assert result["space_key"] == "HR"
        assert result["title"] == "Onboarding"

    def test_tiny_url(self):
        result = parse_confluence_url("http://wiki.example.com/x/AbCd")
        assert result["page_id"] is not None
        assert result["page_id"].startswith("tinyurl:")

    def test_direct_page_id(self):
        result = parse_confluence_url("http://wiki.example.com/pages/98765")
        assert result["page_id"] == "98765"

    def test_encoded_title(self):
        result = parse_confluence_url(
            "http://wiki.example.com/display/SPACE/My%20Encoded%20Title"
        )
        assert result["title"] == "My Encoded Title"

    def test_unrecognized_url_returns_none(self):
        result = parse_confluence_url("http://wiki.example.com/unknown/path")
        assert result["page_id"] is None
        assert result["space_key"] is None
        assert result["title"] is None

    def test_numeric_fallback(self):
        result = parse_confluence_url("http://wiki.example.com/some/path/12345/rest")
        assert result["page_id"] == "12345"
