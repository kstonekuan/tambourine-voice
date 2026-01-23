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
from types.providers import LLMProviderSelection, STTProviderSelection
from typing import Annotated, Any, Literal

from loguru import logger
from pydantic import BaseModel, Field, ValidationError

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
    """Client request to start recording audio."""

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


ClientMessage = Annotated[
    StartRecordingMessage | StopRecordingMessage | SetSTTProviderMessage | SetLLMProviderMessage,
    Field(discriminator="type"),
]


def parse_client_message(raw: dict[str, Any]) -> ClientMessage | None:
    """Parse client message with forward compatibility.

    Returns None for unknown message types (logs at debug level).
    This allows the server to gracefully ignore messages from newer clients
    that use message types not yet supported by this server version.
    """
    try:
        return ClientMessage.model_validate(raw)
    except ValidationError:
        logger.debug(f"Unknown client message type: {raw.get('type')}")
        return None


# =============================================================================
# Server Messages
# =============================================================================


class RecordingCompleteMessage(BaseModel):
    """Server notification that recording processing is complete (no content)."""

    type: Literal["recording-complete"] = "recording-complete"
    hasContent: bool = False


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
    RecordingCompleteMessage | ConfigUpdatedMessage | ConfigErrorMessage,
    Field(discriminator="type"),
]


# =============================================================================
# Legacy String Constants (for backward compatibility during migration)
# =============================================================================


class ClientMessageType(StrEnum):
    """String constants for client message types.

    Deprecated: Use the Pydantic message models directly instead.
    """

    START_RECORDING = "start-recording"
    STOP_RECORDING = "stop-recording"
    SET_STT_PROVIDER = "set-stt-provider"
    SET_LLM_PROVIDER = "set-llm-provider"


class ServerMessageType(StrEnum):
    """String constants for server message types.

    Deprecated: Use the Pydantic message models directly instead.
    """

    RECORDING_COMPLETE = "recording-complete"
    CONFIG_UPDATED = "config-updated"
    CONFIG_ERROR = "config-error"
