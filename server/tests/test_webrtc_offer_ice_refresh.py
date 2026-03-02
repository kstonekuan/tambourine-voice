from dataclasses import dataclass
from types import SimpleNamespace
from typing import Any

from fastapi.testclient import TestClient
from pipecat.transports.smallwebrtc.connection import IceServer

import main
from processors.client_manager import ClientConnectionManager


class _RecordingWebRTCHandler:
    def __init__(self) -> None:
        self.updated_ice_servers_history: list[list[IceServer] | None] = []
        self.update_call_count = 0
        self.update_call_count_at_handle_request: list[int] = []

    def update_ice_servers(self, ice_servers: list[IceServer] | None = None) -> None:
        self.update_call_count += 1
        self.updated_ice_servers_history.append(ice_servers)

    async def handle_web_request(
        self, request: Any, webrtc_connection_callback: Any
    ) -> dict[str, str]:
        _ = request
        _ = webrtc_connection_callback
        self.update_call_count_at_handle_request.append(self.update_call_count)
        return {"sdp": "answer-sdp", "type": "answer"}

    async def close(self) -> None:
        pass


@dataclass
class _TestSettings:
    turn_server_url: str | None = None
    turn_shared_secret: str | None = None
    turn_credential_ttl: int = 3600


def _build_test_client(
    settings: _TestSettings,
) -> tuple[TestClient, ClientConnectionManager, _RecordingWebRTCHandler]:
    client_manager = ClientConnectionManager()
    webrtc_handler = _RecordingWebRTCHandler()
    main.app.state.services = SimpleNamespace(
        settings=settings,
        client_manager=client_manager,
        active_pipeline_tasks=set(),
        webrtc_handler=webrtc_handler,
        available_stt_providers=[],
        available_llm_providers=[],
    )
    return TestClient(main.app), client_manager, webrtc_handler


def test_webrtc_offer_refreshes_handler_ice_servers_on_every_request(monkeypatch: Any) -> None:
    settings = _TestSettings()
    test_client, client_manager, webrtc_handler = _build_test_client(settings=settings)
    registered_client_uuid = client_manager.generate_and_register_uuid()

    generated_ice_server_batches = [
        [IceServer(urls="stun:stun-one.example.com:3478")],
        [IceServer(urls="stun:stun-two.example.com:3478")],
    ]
    build_ice_servers_call_count = 0

    def fake_build_ice_servers(passed_settings: _TestSettings) -> list[IceServer]:
        nonlocal build_ice_servers_call_count
        assert passed_settings is settings
        generated_ice_servers = generated_ice_server_batches[build_ice_servers_call_count]
        build_ice_servers_call_count += 1
        return generated_ice_servers

    monkeypatch.setattr(main, "build_ice_servers", fake_build_ice_servers)

    initial_offer_payload = {
        "sdp": "v=0\r\n",
        "type": "offer",
        "pc_id": "pc-1",
        "requestData": {"clientUUID": registered_client_uuid},
    }

    with test_client as client:
        first_response = client.post("/api/offer", json=initial_offer_payload)
        second_response = client.post(
            "/api/offer",
            json={
                **initial_offer_payload,
                "pc_id": "pc-2",
            },
        )

    assert first_response.status_code == 200
    assert second_response.status_code == 200
    assert first_response.json() == {"sdp": "answer-sdp", "type": "answer"}
    assert second_response.json() == {"sdp": "answer-sdp", "type": "answer"}
    assert build_ice_servers_call_count == 2
    assert webrtc_handler.updated_ice_servers_history == generated_ice_server_batches
    assert webrtc_handler.update_call_count_at_handle_request == [1, 2]
