from datetime import UTC, datetime

from protocol.messages import StartRecordingMessage, parse_client_message


def build_focus_context_payload() -> dict[str, object]:
    return {
        "focused_application": {"display_name": "Code"},
        "focused_window": {"title": "main.py"},
        "event_source": "polling",
        "confidence_level": "high",
        "privacy_filtered": True,
        "captured_at": datetime.now(tz=UTC).isoformat(),
    }


def test_start_recording_focus_context_for_recording_returns_snapshot() -> None:
    parsed_message = parse_client_message(
        {
            "type": "start-recording",
            "data": {"focus_context": build_focus_context_payload()},
        }
    )
    assert isinstance(parsed_message, StartRecordingMessage)

    focus_context_snapshot = parsed_message.focus_context_for_recording()
    assert focus_context_snapshot is not None
    assert focus_context_snapshot.focused_application is not None
    assert focus_context_snapshot.focused_application.display_name == "Code"


def test_start_recording_focus_context_for_recording_returns_none_for_empty_data() -> None:
    parsed_message = parse_client_message({"type": "start-recording", "data": {}})
    assert isinstance(parsed_message, StartRecordingMessage)

    assert parsed_message.focus_context_for_recording() is None


def test_start_recording_focus_context_for_recording_returns_none_for_missing_data() -> None:
    parsed_message = parse_client_message({"type": "start-recording"})
    assert isinstance(parsed_message, StartRecordingMessage)

    assert parsed_message.focus_context_for_recording() is None
