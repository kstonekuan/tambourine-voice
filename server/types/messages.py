"""Message type enums for RTVI client-server communication.

These enums provide type-safe message handling with IDE autocomplete
and exhaustiveness checking via pattern matching.
"""

from enum import StrEnum


class ClientMessageType(StrEnum):
    """Messages received from the RTVI client."""

    START_RECORDING = "start-recording"
    STOP_RECORDING = "stop-recording"
    SET_STT_PROVIDER = "set-stt-provider"
    SET_LLM_PROVIDER = "set-llm-provider"


class ServerMessageType(StrEnum):
    """Messages sent to the RTVI client."""

    RECORDING_COMPLETE = "recording-complete"
    CONFIG_UPDATED = "config-updated"
    CONFIG_ERROR = "config-error"
