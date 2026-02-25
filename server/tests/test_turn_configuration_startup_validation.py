from types import SimpleNamespace
from typing import Any

import main
from config.settings import Settings


class _CapturingLogger:
    def __init__(self) -> None:
        self.info_messages: list[str] = []
        self.error_messages: list[str] = []

    def info(self, message: str) -> None:
        self.info_messages.append(message)

    def error(self, message: str) -> None:
        self.error_messages.append(message)


def test_initialize_services_fails_when_turn_url_is_missing_shared_secret(
    monkeypatch: Any,
) -> None:
    turn_misconfigured_settings = Settings.model_construct(
        turn_server_url="turn:turn.example.com:3478",
        turn_shared_secret=None,
        turn_credential_ttl=3600,
    )

    fake_logger = _CapturingLogger()
    monkeypatch.setattr(main, "logger", fake_logger)

    def fake_get_available_stt_providers(_settings: Any) -> list[Any]:
        return [SimpleNamespace(value="deepgram")]

    def fake_get_available_llm_providers(_settings: Any) -> list[Any]:
        return [SimpleNamespace(value="openai")]

    build_ice_servers_call_count = 0

    def fake_build_ice_servers(_settings: Any) -> list[Any]:
        nonlocal build_ice_servers_call_count
        build_ice_servers_call_count += 1
        return []

    monkeypatch.setattr(main, "get_available_stt_providers", fake_get_available_stt_providers)
    monkeypatch.setattr(main, "get_available_llm_providers", fake_get_available_llm_providers)
    monkeypatch.setattr(main, "build_ice_servers", fake_build_ice_servers)

    initialized_services = main.initialize_services(turn_misconfigured_settings)

    assert initialized_services is None
    assert build_ice_servers_call_count == 0
    assert fake_logger.error_messages == [
        "TURN_SERVER_URL is set but TURN_SHARED_SECRET is missing. "
        "Refusing to start with partial TURN configuration."
    ]


def test_initialize_services_fails_when_turn_shared_secret_is_missing_url(
    monkeypatch: Any,
) -> None:
    turn_misconfigured_settings = Settings.model_construct(
        turn_server_url=None,
        turn_shared_secret="test-shared-secret",
        turn_credential_ttl=3600,
    )

    fake_logger = _CapturingLogger()
    monkeypatch.setattr(main, "logger", fake_logger)

    def fake_get_available_stt_providers(_settings: Any) -> list[Any]:
        return [SimpleNamespace(value="deepgram")]

    def fake_get_available_llm_providers(_settings: Any) -> list[Any]:
        return [SimpleNamespace(value="openai")]

    build_ice_servers_call_count = 0

    def fake_build_ice_servers(_settings: Any) -> list[Any]:
        nonlocal build_ice_servers_call_count
        build_ice_servers_call_count += 1
        return []

    monkeypatch.setattr(main, "get_available_stt_providers", fake_get_available_stt_providers)
    monkeypatch.setattr(main, "get_available_llm_providers", fake_get_available_llm_providers)
    monkeypatch.setattr(main, "build_ice_servers", fake_build_ice_servers)

    initialized_services = main.initialize_services(turn_misconfigured_settings)

    assert initialized_services is None
    assert build_ice_servers_call_count == 0
    assert fake_logger.error_messages == [
        "TURN_SHARED_SECRET is set but TURN_SERVER_URL is missing. "
        "Refusing to start with partial TURN configuration."
    ]
