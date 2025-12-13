"""FastAPI configuration router for Tambourine settings.

This module provides REST endpoints for:
- Getting default prompt sections
- Getting available providers (static configuration)

All runtime pipeline configuration is now handled via WebRTC data channel
through the ConfigurationProcessor. This file only exposes static
configuration data that doesn't require pipeline access.
"""

from __future__ import annotations

from typing import Any

from fastapi import APIRouter
from pydantic import BaseModel

from processors.llm_cleanup import (
    ADVANCED_PROMPT_DEFAULT,
    DICTIONARY_PROMPT_DEFAULT,
    MAIN_PROMPT_DEFAULT,
)
from services.provider_registry import (
    LLMProviderId,
    STTProviderId,
    get_llm_provider_labels,
    get_stt_provider_labels,
)

# Create router for config endpoints
config_router = APIRouter()

# Store available providers (set at startup, static during runtime)
_available_stt_providers: list[STTProviderId] = []
_available_llm_providers: list[LLMProviderId] = []


def set_available_providers(
    stt_services: dict[STTProviderId, Any],
    llm_services: dict[LLMProviderId, Any],
) -> None:
    """Set the list of available providers (called once at startup).

    Args:
        stt_services: Dictionary of available STT services
        llm_services: Dictionary of available LLM services
    """
    global _available_stt_providers, _available_llm_providers
    _available_stt_providers = list(stt_services.keys())
    _available_llm_providers = list(llm_services.keys())


# =============================================================================
# Prompt Section Models and Endpoints
# =============================================================================


class DefaultSectionsResponse(BaseModel):
    """Response with default prompts for each section."""

    main: str
    advanced: str
    dictionary: str


@config_router.get("/api/prompt/sections/default", response_model=DefaultSectionsResponse)
async def get_default_sections() -> DefaultSectionsResponse:
    """Get default prompts for each section."""
    return DefaultSectionsResponse(
        main=MAIN_PROMPT_DEFAULT,
        advanced=ADVANCED_PROMPT_DEFAULT,
        dictionary=DICTIONARY_PROMPT_DEFAULT,
    )


# =============================================================================
# Provider Information Endpoints
# =============================================================================


class ProviderInfo(BaseModel):
    """Information about a provider."""

    value: str
    label: str


class AvailableProvidersResponse(BaseModel):
    """Response listing available providers."""

    stt: list[ProviderInfo]
    llm: list[ProviderInfo]


@config_router.get("/api/providers/available", response_model=AvailableProvidersResponse)
async def get_available_providers() -> AvailableProvidersResponse:
    """Get list of available STT and LLM providers (those with API keys configured).

    This endpoint returns static data configured at server startup.
    """
    stt_labels = get_stt_provider_labels()
    llm_labels = get_llm_provider_labels()

    stt_providers = [
        ProviderInfo(
            value=provider_id.value,
            label=stt_labels.get(provider_id, provider_id.value),
        )
        for provider_id in _available_stt_providers
    ]

    llm_providers = [
        ProviderInfo(
            value=provider_id.value,
            label=llm_labels.get(provider_id, provider_id.value),
        )
        for provider_id in _available_llm_providers
    ]

    return AvailableProvidersResponse(stt=stt_providers, llm=llm_providers)
