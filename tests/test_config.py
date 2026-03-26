"""Unit tests for config.py."""

import os
import pytest
from unittest.mock import patch


def _env(**kwargs):
    """Return a minimal valid environment with overrides."""
    base = {
        "CONFLUENCE_URL": "https://wiki.example.com",
        "CONFLUENCE_TOKEN": "mytoken",
        "ATLASSIAN_DEPLOYMENT": "datacenter",
        "MCP_TRANSPORT": "http",
    }
    base.update(kwargs)
    return base


class TestLoadConfig:
    def test_basic_confluence_only(self):
        with patch.dict(os.environ, _env(), clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert config.confluence.enabled is True
        assert config.confluence.url == "https://wiki.example.com"
        assert config.confluence.token == "mytoken"
        assert config.jira.enabled is False
        assert config.bitbucket.enabled is False

    def test_global_fallback_credentials(self):
        env = {
            "ATLASSIAN_URL": "https://myorg.atlassian.net",
            "ATLASSIAN_TOKEN": "global_token",
            "ATLASSIAN_DEPLOYMENT": "cloud",
            "MCP_TRANSPORT": "http",
        }
        with patch.dict(os.environ, env, clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert config.confluence.url == "https://myorg.atlassian.net"
        assert config.confluence.token == "global_token"
        assert config.jira.url == "https://myorg.atlassian.net"
        assert config.bitbucket.url == "https://myorg.atlassian.net"

    def test_service_specific_overrides_global(self):
        env = {
            "ATLASSIAN_URL": "https://global.example.com",
            "ATLASSIAN_TOKEN": "global_token",
            "CONFLUENCE_URL": "https://wiki.example.com",
            "CONFLUENCE_TOKEN": "conf_token",
            "MCP_TRANSPORT": "http",
            "ATLASSIAN_DEPLOYMENT": "datacenter",
        }
        with patch.dict(os.environ, env, clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert config.confluence.url == "https://wiki.example.com"
        assert config.confluence.token == "conf_token"
        assert config.jira.url == "https://global.example.com"
        assert config.jira.token == "global_token"

    def test_ssl_verify_false(self):
        env = _env(**{"CONFLUENCE_SSL_VERIFY": "false"})
        with patch.dict(os.environ, env, clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert config.confluence.ssl_verify is False

    def test_ssl_verify_ca_bundle(self):
        env = _env(**{"CONFLUENCE_CA_BUNDLE": "/etc/ssl/certs/ca.pem"})
        with patch.dict(os.environ, env, clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert config.confluence.ssl_verify == "/etc/ssl/certs/ca.pem"

    def test_deployment_type_cloud(self):
        env = _env(**{"ATLASSIAN_DEPLOYMENT": "cloud"})
        with patch.dict(os.environ, env, clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert config.deployment_type == "cloud"

    def test_trailing_slash_stripped(self):
        env = _env(**{"CONFLUENCE_URL": "https://wiki.example.com/"})
        with patch.dict(os.environ, env, clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert not config.confluence.url.endswith("/")

    def test_timeout_and_rate_limit_parsed(self):
        env = _env(**{"CONFLUENCE_TIMEOUT": "60", "CONFLUENCE_RATE_LIMIT": "10"})
        with patch.dict(os.environ, env, clear=True):
            from importlib import reload
            import config as cfg_module
            reload(cfg_module)
            config = cfg_module.load_config()

        assert config.confluence.timeout == 60
        assert config.confluence.rate_limit == 10


class TestAtlassianConfigValidate:
    def _make_service(self, **kwargs):
        from config import ServiceConfig
        defaults = {
            "url": "https://example.com",
            "token": "tok",
            "username": None,
            "password": None,
            "ssl_verify": True,
            "timeout": 30,
            "rate_limit": 5,
            "enabled": True,
        }
        defaults.update(kwargs)
        return ServiceConfig(**defaults)

    def _make_config(self, **kwargs):
        from config import AtlassianConfig, ServiceConfig
        disabled_svc = ServiceConfig(
            url="", token=None, username=None, password=None,
            ssl_verify=True, timeout=30, rate_limit=5, enabled=False,
        )
        defaults = dict(
            deployment_type="datacenter",
            confluence=self._make_service(),
            jira=disabled_svc,
            bitbucket=disabled_svc,
            transport="http",
            host="0.0.0.0",
            port=8000,
            max_content_length=50000,
            default_search_limit=10,
            log_level="INFO",
        )
        defaults.update(kwargs)
        return AtlassianConfig(**defaults)

    def test_valid_config_no_errors(self):
        config = self._make_config()
        assert config.validate() == []

    def test_invalid_deployment_type(self):
        config = self._make_config(deployment_type="on-prem")
        errors = config.validate()
        assert any("ATLASSIAN_DEPLOYMENT" in e for e in errors)

    def test_invalid_transport(self):
        config = self._make_config(transport="grpc")
        errors = config.validate()
        assert any("MCP_TRANSPORT" in e for e in errors)

    def test_no_services_enabled(self):
        from config import ServiceConfig
        disabled_svc = ServiceConfig(
            url="", token=None, username=None, password=None,
            ssl_verify=True, timeout=30, rate_limit=5, enabled=False,
        )
        config = self._make_config(confluence=disabled_svc, jira=disabled_svc, bitbucket=disabled_svc)
        errors = config.validate()
        assert any("No services" in e for e in errors)

    def test_service_without_credentials(self):
        from config import ServiceConfig
        no_creds_svc = ServiceConfig(
            url="https://example.com", token=None, username=None, password=None,
            ssl_verify=True, timeout=30, rate_limit=5, enabled=True,
        )
        config = self._make_config(confluence=no_creds_svc)
        errors = config.validate()
        assert any("Confluence" in e for e in errors)

    def test_auth_method_token(self):
        svc = self._make_service(token="mytoken")
        assert svc.auth_method == "token"

    def test_auth_method_basic(self):
        svc = self._make_service(token=None, username="user", password="pass")
        assert svc.auth_method == "basic"

    def test_auth_method_none(self):
        svc = self._make_service(token=None, username=None, password=None)
        assert svc.auth_method == "none"
