"""Pydantic models for RTVI client-server message communication.

This module provides type-safe message handling with:
- Discriminated unions for exhaustive pattern matching
- Clear distinction between recording and config message types
- Typed setting names and values for configuration messages

Message flow:
- Client → Server: RecordingMessage | ConfigMessage (via RTVI data channel)
- Server → Client: ServerMessage (via RTVIServerMessageFrame)
"""

from enum import StrEnum
from typing import Annotated, Any, Literal

from loguru import logger
from pydantic import BaseModel, Field, RootModel, ValidationError

from protocol.providers import LLMProviderSelection, STTProviderSelection

# =============================================================================
# Setting Names (used in config-updated and config-error responses)
# =============================================================================


class SettingName(StrEnum):
    """Valid setting names for configuration messages."""

    STT_PROVIDER = "stt-provider"
    LLM_PROVIDER = "llm-provider"
    PROMPT_SECTIONS = "prompt-sections"
    STT_TIMEOUT = "stt-timeout"


# =============================================================================
# Client Messages - Recording
# =============================================================================


class StartRecordingMessage(BaseModel):
    """Client request to start recording audio.

    This is a simple marker message with no data payload.
    LLM formatting is controlled globally via the /api/config/llm-formatting endpoint.
    """

    type: Literal["start-recording"]


class StopRecordingMessage(BaseModel):
    """Client request to stop recording and process audio."""

    type: Literal["stop-recording"]


RecordingMessage = Annotated[
    StartRecordingMessage | StopRecordingMessage,
    Field(discriminator="type"),
]


# =============================================================================
# Client Messages - Configuration (Provider Switching)
# =============================================================================


class SetSTTProviderData(BaseModel):
    """Data payload for set-stt-provider message."""

    provider: STTProviderSelection


class SetLLMProviderData(BaseModel):
    """Data payload for set-llm-provider message."""

    provider: LLMProviderSelection


class SetSTTProviderMessage(BaseModel):
    """Client request to switch STT provider."""

    type: Literal["set-stt-provider"]
    data: SetSTTProviderData


class SetLLMProviderMessage(BaseModel):
    """Client request to switch LLM provider."""

    type: Literal["set-llm-provider"]
    data: SetLLMProviderData


ConfigMessage = Annotated[
    SetSTTProviderMessage | SetLLMProviderMessage,
    Field(discriminator="type"),
]


# =============================================================================
# Combined Client Message Type
# =============================================================================


# Type alias for all known client message types
_ClientMessageUnion = (
    StartRecordingMessage | StopRecordingMessage | SetSTTProviderMessage | SetLLMProviderMessage
)


class ClientMessage(RootModel[Annotated[_ClientMessageUnion, Field(discriminator="type")]]):
    """Discriminated union wrapper for all client messages.

    This is a proper Pydantic model that supports model_validate().
    Access the underlying typed message via the .root attribute.
    """

    pass


class UnknownClientMessage(BaseModel):
    """Unknown client message type (forward compatibility).

    Preserves the raw message data for debugging, similar to
    OtherSTTProvider/OtherLLMProvider pattern in providers.py.
    """

    type: str  # The actual unknown type string
    raw: dict[str, Any]  # Full original message for debugging


def parse_client_message(raw: dict[str, Any]) -> _ClientMessageUnion | UnknownClientMessage:
    """Parse client message with forward compatibility.

    Returns UnknownClientMessage for unknown types (never None).
    This allows exhaustive pattern matching while preserving raw data
    for debugging purposes.
    """
    try:
        wrapper = ClientMessage.model_validate(raw)
        return wrapper.root  # Return the actual message, not the wrapper
    except ValidationError:
        logger.debug(f"Unknown client message type: {raw.get('type')}")
        return UnknownClientMessage(type=raw.get("type", ""), raw=raw)


# =============================================================================
# Server Messages
# =============================================================================


class RecordingCompleteMessage(BaseModel):
    """Server notification that recording processing is complete (no content)."""

    type: Literal["recording-complete"] = "recording-complete"
    hasContent: bool = False


class RawTranscriptionMessage(BaseModel):
    """Server message containing raw transcription (LLM bypassed).

    Sent when LLM formatting is disabled via the config API.
    Contains the unformatted transcription directly from STT.
    """

    type: Literal["raw-transcription"] = "raw-transcription"
    text: str


class ConfigUpdatedMessage(BaseModel):
    """Server notification that a setting was updated successfully."""

    type: Literal["config-updated"] = "config-updated"
    setting: SettingName
    value: Any
    success: Literal[True] = True


class ConfigErrorMessage(BaseModel):
    """Server notification that a configuration update failed."""

    type: Literal["config-error"] = "config-error"
    setting: SettingName
    error: str


ServerMessage = Annotated[
    RecordingCompleteMessage | RawTranscriptionMessage | ConfigUpdatedMessage | ConfigErrorMessage,
    Field(discriminator="type"),
]
