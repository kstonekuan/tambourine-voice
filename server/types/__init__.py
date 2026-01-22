"""Type definitions for the server module."""

from types.messages import (
    ClientMessage,
    ClientMessageType,
    ConfigErrorMessage,
    ConfigMessage,
    ConfigUpdatedMessage,
    RecordingCompleteMessage,
    RecordingMessage,
    ServerMessage,
    ServerMessageType,
    SetLLMProviderData,
    SetLLMProviderMessage,
    SetSTTProviderData,
    SetSTTProviderMessage,
    SettingName,
    StartRecordingMessage,
    StopRecordingMessage,
)
from types.providers import (
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
    "ClientMessageType",
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
    "ServerMessageType",
    "SetLLMProviderData",
    "SetLLMProviderMessage",
    "SetSTTProviderData",
    "SetSTTProviderMessage",
    "SettingName",
    "StartRecordingMessage",
    "StopRecordingMessage",
]
