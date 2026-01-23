"""Type definitions for the server module."""

from protocol.messages import (
    ClientMessage,
    ConfigErrorMessage,
    ConfigMessage,
    ConfigUpdatedMessage,
    RecordingCompleteMessage,
    RecordingMessage,
    ServerMessage,
    SetLLMProviderData,
    SetLLMProviderMessage,
    SetSTTProviderData,
    SetSTTProviderMessage,
    SettingName,
    StartRecordingMessage,
    StopRecordingMessage,
    UnknownClientMessage,
)
from protocol.providers import (
    AutoProvider,
    KnownLLMProvider,
    KnownSTTProvider,
    LLMProviderSelection,
    OtherLLMProvider,
    OtherSTTProvider,
    STTProviderSelection,
)

__all__ = [
    # Provider types
    "AutoProvider",
    # Message types
    "ClientMessage",
    "ConfigErrorMessage",
    "ConfigMessage",
    "ConfigUpdatedMessage",
    "KnownLLMProvider",
    "KnownSTTProvider",
    "LLMProviderSelection",
    "OtherLLMProvider",
    "OtherSTTProvider",
    "RecordingCompleteMessage",
    "RecordingMessage",
    "STTProviderSelection",
    "ServerMessage",
    "SetLLMProviderData",
    "SetLLMProviderMessage",
    "SetSTTProviderData",
    "SetSTTProviderMessage",
    "SettingName",
    "StartRecordingMessage",
    "StopRecordingMessage",
    "UnknownClientMessage",
]
