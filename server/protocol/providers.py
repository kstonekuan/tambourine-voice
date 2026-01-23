"""Typed provider selection discriminated unions.

These types provide type-safe provider selection with three modes:
- AutoProvider: Use server's configured default provider
- KnownSTTProvider/KnownLLMProvider: Known provider from the enum
- OtherSTTProvider/OtherLLMProvider: Unknown provider (forward compatibility)

The discriminated union pattern enables exhaustive pattern matching
and type-safe provider handling.
"""

from typing import Annotated, Literal

from pydantic import BaseModel, Field

from services.provider_registry import LLMProviderId, STTProviderId


class AutoProvider(BaseModel):
    """Auto mode: use server's configured default provider."""

    mode: Literal["auto"]


class KnownSTTProvider(BaseModel):
    """Known STT provider from the enum."""

    mode: Literal["known"]
    provider_id: STTProviderId


class OtherSTTProvider(BaseModel):
    """Unknown STT provider (forward compatibility)."""

    mode: Literal["other"]
    provider_id: str


class KnownLLMProvider(BaseModel):
    """Known LLM provider from the enum."""

    mode: Literal["known"]
    provider_id: LLMProviderId


class OtherLLMProvider(BaseModel):
    """Unknown LLM provider (forward compatibility)."""

    mode: Literal["other"]
    provider_id: str


STTProviderSelection = Annotated[
    AutoProvider | KnownSTTProvider | OtherSTTProvider,
    Field(discriminator="mode"),
]

LLMProviderSelection = Annotated[
    AutoProvider | KnownLLMProvider | OtherLLMProvider,
    Field(discriminator="mode"),
]


def parse_stt_provider_selection(provider_value: str | None) -> STTProviderSelection | None:
    """Parse a provider string into an STTProviderSelection.

    Args:
        provider_value: The provider ID string (e.g., "deepgram", "openai", "auto")

    Returns:
        The parsed STTProviderSelection, or None if provider_value is None/empty
    """
    if not provider_value:
        return None

    if provider_value == "auto":
        return AutoProvider(mode="auto")

    # Try to parse as known provider
    try:
        provider_id = STTProviderId(provider_value)
        return KnownSTTProvider(mode="known", provider_id=provider_id)
    except ValueError:
        # Unknown provider - forward compatibility
        return OtherSTTProvider(mode="other", provider_id=provider_value)


def parse_llm_provider_selection(provider_value: str | None) -> LLMProviderSelection | None:
    """Parse a provider string into an LLMProviderSelection.

    Args:
        provider_value: The provider ID string (e.g., "openai", "anthropic", "auto")

    Returns:
        The parsed LLMProviderSelection, or None if provider_value is None/empty
    """
    if not provider_value:
        return None

    if provider_value == "auto":
        return AutoProvider(mode="auto")

    # Try to parse as known provider
    try:
        provider_id = LLMProviderId(provider_value)
        return KnownLLMProvider(mode="known", provider_id=provider_id)
    except ValueError:
        # Unknown provider - forward compatibility
        return OtherLLMProvider(mode="other", provider_id=provider_value)
