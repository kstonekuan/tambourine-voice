"""Pydantic models for RTVI client-server message communication.

This module provides type-safe message handling with:
- Discriminated unions for exhaustive pattern matching
- Clear distinction between recording and config message types
- Typed setting names and values for configuration messages

Message flow:
- Client → Server: RecordingMessage | ConfigMessage (via RTVI data channel)
- Server → Client: ServerMessage (via RTVIServerMessageFrame)
"""

from collections.abc import Mapping
from enum import StrEnum
from typing import Annotated, Literal

from loguru import logger
from pydantic import BaseModel, ConfigDict, Field, RootModel, ValidationError, field_validator

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
# Active App Context Types (Client -> Server)
# =============================================================================


class FocusEventSource(StrEnum):
    """Source of active app context data."""

    POLLING = "polling"
    ACCESSIBILITY = "accessibility"
    UIA = "uia"
    UNKNOWN = "unknown"


class FocusConfidenceLevel(StrEnum):
    """Confidence level of active app context data."""

    HIGH = "high"
    MEDIUM = "medium"
    LOW = "low"


class FocusedApplication(BaseModel):
    """Focused application details."""

    display_name: str
    bundle_id: str | None = None
    process_path: str | None = None


class FocusedWindow(BaseModel):
    """Focused window details."""

    title: str


class FocusedBrowserTab(BaseModel):
    """Focused browser tab details (best-effort)."""

    title: str | None = None
    origin: str | None = None
    browser: str | None = None


class ActiveAppContextSnapshot(BaseModel):
    """Snapshot of current active app context."""

    model_config = ConfigDict(extra="ignore")

    focused_application: FocusedApplication | None = None
    focused_window: FocusedWindow | None = None
    focused_browser_tab: FocusedBrowserTab | None = None
    event_source: FocusEventSource = FocusEventSource.UNKNOWN
    confidence_level: FocusConfidenceLevel = FocusConfidenceLevel.LOW
    captured_at: str


# =============================================================================
# Client Messages - Recording
# =============================================================================


class StartRecordingData(BaseModel):
    """Optional data payload for start-recording message."""

    model_config = ConfigDict(extra="ignore")

    active_app_context: ActiveAppContextSnapshot | None = None

    @field_validator("active_app_context", mode="before")
    @classmethod
    def parse_active_app_context_or_clear(
        cls,
        raw_active_app_context: object,
    ) -> ActiveAppContextSnapshot | None:
        """Treat malformed optional active app context as absent metadata."""
        if raw_active_app_context is None:
            return None
        if isinstance(raw_active_app_context, ActiveAppContextSnapshot):
            return raw_active_app_context
        if not isinstance(raw_active_app_context, Mapping):
            logger.debug("Ignoring non-mapping active_app_context payload")
            return None
        try:
            return ActiveAppContextSnapshot.model_validate(raw_active_app_context)
        except ValidationError:
            logger.debug("Ignoring malformed active_app_context payload")
            return None


class StartRecordingMessage(BaseModel):
    """Client request to start recording audio.

    LLM formatting is controlled globally via the /api/config/llm-formatting endpoint.
    """

    type: Literal["start-recording"]
    data: StartRecordingData | None = None

    def active_app_context_for_recording(self) -> ActiveAppContextSnapshot | None:
        """Return active app context for this recording, or None to explicitly clear it."""
        if self.data is None:
            return None
        return self.data.active_app_context


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
    raw: dict[str, object]  # Full original message for debugging


class RTVIClientMessageEnvelope(BaseModel):
    """Raw RTVI client message envelope.

    Validates message shape from object attributes to avoid untyped attribute access
    in event callbacks.
    """

    model_config = ConfigDict(extra="ignore", from_attributes=True)

    type: str
    data: object | None = None

    def to_client_message_payload(self) -> dict[str, object]:
        """Normalize envelope fields into parser-ready payload."""
        normalized_data = (
            {str(key): value for key, value in self.data.items()}
            if isinstance(self.data, Mapping)
            else {}
        )
        return {
            "type": self.type,
            "data": normalized_data,
        }


def parse_rtvi_client_message_payload(raw_message: object) -> dict[str, object] | None:
    """Parse a raw RTVI message object into a normalized payload.

    Returns None for invalid envelope shapes.
    """
    try:
        envelope = RTVIClientMessageEnvelope.model_validate(raw_message)
    except ValidationError:
        return None
    return envelope.to_client_message_payload()


def parse_client_message(raw: Mapping[str, object]) -> _ClientMessageUnion | UnknownClientMessage:
    """Parse client message with forward compatibility.

    Returns UnknownClientMessage for unknown types (never None).
    This allows exhaustive pattern matching while preserving raw data
    for debugging purposes.
    """
    try:
        wrapper = ClientMessage.model_validate(raw)
        return wrapper.root  # Return the actual message, not the wrapper
    except ValidationError:
        raw_message_type = raw.get("type")
        unknown_message_type = raw_message_type if isinstance(raw_message_type, str) else ""
        logger.debug(f"Unknown client message type: {unknown_message_type}")
        return UnknownClientMessage(type=unknown_message_type, raw=dict(raw))


# =============================================================================
# Server Messages
# =============================================================================


class EmptyTranscriptMessage(BaseModel):
    """Server notification that recording processing is complete (no content)."""

    type: Literal["recording-complete-with-zero-words"] = "recording-complete-with-zero-words"


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
    value: STTProviderSelection | LLMProviderSelection
    success: Literal[True] = True


class ConfigErrorMessage(BaseModel):
    """Server notification that a configuration update failed."""

    type: Literal["config-error"] = "config-error"
    setting: SettingName
    error: str


ServerMessage = Annotated[
    EmptyTranscriptMessage | RawTranscriptionMessage | ConfigUpdatedMessage | ConfigErrorMessage,
    Field(discriminator="type"),
]
