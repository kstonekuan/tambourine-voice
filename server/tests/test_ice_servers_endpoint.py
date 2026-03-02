from dataclasses import dataclass
from types import SimpleNamespace

from fastapi.testclient import TestClient

from main import app
from processors.client_manager import ClientConnectionManager


class _NoopWebRTCHandler:
    async def close(self) -> None:
        pass


@dataclass
class _TestSettings:
    turn_server_url: str | None = None
    turn_shared_secret: str | None = None
    turn_credential_ttl: int = 3600


def _build_test_client(settings: _TestSettings) -> tuple[TestClient, ClientConnectionManager]:
    client_manager = ClientConnectionManager()
    app.state.services = SimpleNamespace(
        settings=settings,
        client_manager=client_manager,
        active_pipeline_tasks=set(),
        webrtc_handler=_NoopWebRTCHandler(),
    )
    return TestClient(app), client_manager


def test_get_ice_servers_rejects_missing_client_uuid_header() -> None:
    test_client, _ = _build_test_client(settings=_TestSettings())
    with test_client as client:
        response = client.get("/api/ice-servers")

    assert response.status_code == 401
    assert response.json() == {"detail": "Client UUID required. Please register first."}


def test_get_ice_servers_rejects_unregistered_client_uuid() -> None:
    test_client, _ = _build_test_client(settings=_TestSettings())
    with test_client as client:
        response = client.get(
            "/api/ice-servers",
            headers={"X-Client-UUID": "not-registered"},
        )

    assert response.status_code == 401
    assert response.json() == {"detail": "Unregistered client UUID. Please register first."}


def test_get_ice_servers_returns_stun_and_turn_for_registered_client() -> None:
    settings = _TestSettings(
        turn_server_url="turn:turn.example.com:3478",
        turn_shared_secret="test-secret",
        turn_credential_ttl=3600,
    )
    test_client, client_manager = _build_test_client(settings=settings)
    registered_client_uuid = client_manager.generate_and_register_uuid()

    with test_client as client:
        response = client.get(
            "/api/ice-servers",
            headers={"X-Client-UUID": registered_client_uuid},
        )

    assert response.status_code == 200
    response_payload = response.json()
    ice_servers = response_payload["ice_servers"]

    assert ice_servers[0] == {
        "urls": "stun:stun.l.google.com:19302",
        "username": None,
        "credential": None,
    }
    assert ice_servers[1]["urls"] == "turn:turn.example.com:3478"
    assert isinstance(ice_servers[1]["username"], str)
    assert isinstance(ice_servers[1]["credential"], str)
    assert ice_servers[1]["username"]
    assert ice_servers[1]["credential"]
