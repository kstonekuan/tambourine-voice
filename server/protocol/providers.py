"""Provider ID enums and selection types.

This module is the single source of truth for provider identifiers:
- STTProviderId: Speech-to-Text provider IDs
- LLMProviderId: Large Language Model provider IDs

Selection types provide type-safe provider selection with three modes:
- AutoProvider: Use server's configured default provider
- KnownSTTProvider/KnownLLMProvider: Known provider from the enum
- OtherSTTProvider/OtherLLMProvider: Unknown provider (forward compatibility)

The discriminated union pattern enables exhaustive pattern matching
and type-safe provider handling.
"""

from enum import StrEnum
from typing import Annotated, Literal

from pydantic import BaseModel, Field

# =============================================================================
# Provider ID Enums - Single source of truth for valid provider identifiers
# =============================================================================


class STTProviderId(StrEnum):
    """Speech-to-Text provider identifiers."""

    SPEECHMATICS = "speechmatics"
    ASSEMBLYAI = "assemblyai"
    AWS = "aws"
    AZURE = "azure"
    CARTESIA = "cartesia"
    DEEPGRAM = "deepgram"
    GOOGLE = "google"
    GROQ = "groq"
    NEMOTRON = "nemotron"
    OPENAI = "openai"
    WHISPER = "whisper"


class LLMProviderId(StrEnum):
    """Large Language Model provider identifiers."""

    ANTHROPIC = "anthropic"
    BEDROCK = "bedrock"
    CEREBRAS = "cerebras"
    GEMINI = "gemini"
    GROQ = "groq"
    OLLAMA = "ollama"
    OPENAI = "openai"
    OPENROUTER = "openrouter"


# =============================================================================
# Provider Selection Types - Used for both input and config response output
# =============================================================================


class AutoProvider(BaseModel):
    """Auto mode: use server's configured default provider."""

    mode: Literal["auto"]


class KnownSTTProvider(BaseModel):
    """Known STT provider from the enum."""

    mode: Literal["known"]
    provider_id: STTProviderId = Field(
        validation_alias="providerId", serialization_alias="providerId"
    )


class OtherSTTProvider(BaseModel):
    """Unknown STT provider (forward compatibility)."""

    mode: Literal["other"]
    provider_id: str = Field(validation_alias="providerId", serialization_alias="providerId")


class KnownLLMProvider(BaseModel):
    """Known LLM provider from the enum."""

    mode: Literal["known"]
    provider_id: LLMProviderId = Field(
        validation_alias="providerId", serialization_alias="providerId"
    )


class OtherLLMProvider(BaseModel):
    """Unknown LLM provider (forward compatibility)."""

    mode: Literal["other"]
    provider_id: str = Field(validation_alias="providerId", serialization_alias="providerId")


STTProviderSelection = Annotated[
    AutoProvider | KnownSTTProvider | OtherSTTProvider,
    Field(discriminator="mode"),
]

LLMProviderSelection = Annotated[
    AutoProvider | KnownLLMProvider | OtherLLMProvider,
    Field(discriminator="mode"),
]


def _parse_provider_selection[
    ProviderIdEnum: StrEnum,
    KnownProvider: BaseModel,
    OtherProvider: BaseModel,
](
    provider_value: str | None,
    provider_enum: type[ProviderIdEnum],
    known_provider_class: type[KnownProvider],
    other_provider_class: type[OtherProvider],
) -> AutoProvider | KnownProvider | OtherProvider | None:
    """Internal helper for parsing provider strings into selection objects.

    Handles "auto", known providers, and unknown providers (forward compatibility).
    """
    if not provider_value:
        return None

    if provider_value == "auto":
        return AutoProvider(mode="auto")

    # Try to parse as known provider
    try:
        provider_id = provider_enum(provider_value)
        return known_provider_class(mode="known", provider_id=provider_id)
    except ValueError:
        # Unknown provider - forward compatibility
        return other_provider_class(mode="other", provider_id=provider_value)


def parse_stt_provider_selection(provider_value: str | None) -> STTProviderSelection | None:
    """Parse a provider string into an STTProviderSelection.

    Args:
        provider_value: The provider ID string (e.g., "deepgram", "openai", "auto")

    Returns:
        The parsed STTProviderSelection, or None if provider_value is None/empty
    """
    return _parse_provider_selection(
        provider_value, STTProviderId, KnownSTTProvider, OtherSTTProvider
    )


def parse_llm_provider_selection(provider_value: str | None) -> LLMProviderSelection | None:
    """Parse a provider string into an LLMProviderSelection.

    Args:
        provider_value: The provider ID string (e.g., "openai", "anthropic", "auto")

    Returns:
        The parsed LLMProviderSelection, or None if provider_value is None/empty
    """
    return _parse_provider_selection(
        provider_value, LLMProviderId, KnownLLMProvider, OtherLLMProvider
    )
