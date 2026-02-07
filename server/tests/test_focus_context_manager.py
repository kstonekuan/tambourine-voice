from datetime import UTC, datetime

from processors.context_manager import DictationContextManager
from protocol.messages import (
    FocusConfidenceLevel,
    FocusContextSnapshot,
    FocusedApplication,
    FocusedWindow,
    FocusEventSource,
)


def build_focus_context_snapshot(captured_at: str) -> FocusContextSnapshot:
    return FocusContextSnapshot(
        focused_application=FocusedApplication(display_name="Code"),
        focused_window=FocusedWindow(title="notes.md"),
        focused_browser_tab=None,
        event_source=FocusEventSource.POLLING,
        confidence_level=FocusConfidenceLevel.HIGH,
        privacy_filtered=True,
        captured_at=captured_at,
    )


def build_fresh_focus_context_snapshot() -> FocusContextSnapshot:
    return build_focus_context_snapshot(datetime.now(tz=UTC).isoformat())


def test_reset_context_for_new_recording_injects_focus_block_for_old_timestamp() -> None:
    context_manager = DictationContextManager()
    context_manager.set_focus_context(build_focus_context_snapshot("2020-01-01T00:00:00+00:00"))
    context_manager.reset_context_for_new_recording()

    messages_with_focus_context = context_manager._context.get_messages()
    assert len(messages_with_focus_context) == 2
    assert any("Focus Context" in str(message) for message in messages_with_focus_context)


def test_reset_context_for_new_recording_injects_focus_block_for_invalid_timestamp() -> None:
    context_manager = DictationContextManager()
    context_manager.set_focus_context(build_focus_context_snapshot("not-a-valid-timestamp"))
    context_manager.reset_context_for_new_recording()

    messages_with_focus_context = context_manager._context.get_messages()
    assert len(messages_with_focus_context) == 2
    assert any("Focus Context" in str(message) for message in messages_with_focus_context)


def test_reset_context_for_new_recording_omits_focus_block_after_explicit_clear() -> None:
    context_manager = DictationContextManager()
    context_manager.set_focus_context(build_fresh_focus_context_snapshot())
    context_manager.reset_context_for_new_recording()

    messages_with_focus_context = context_manager._context.get_messages()
    assert len(messages_with_focus_context) == 2
    assert any("Focus Context" in str(message) for message in messages_with_focus_context)

    context_manager.set_focus_context(None)
    context_manager.reset_context_for_new_recording()

    messages_after_focus_context_clear = context_manager._context.get_messages()
    assert len(messages_after_focus_context_clear) == 1
    assert all(
        "Focus Context" not in str(message) for message in messages_after_focus_context_clear
    )
