"""FastAPI configuration router for Tambourine settings."""

from __future__ import annotations

import asyncio
from typing import TYPE_CHECKING, Any

from fastapi import APIRouter
from loguru import logger
from pipecat.frames.frames import ManuallySwitchServiceFrame
from pipecat.processors.frame_processor import FrameDirection
from pydantic import BaseModel

from processors.llm_cleanup import (
    ADVANCED_PROMPT_DEFAULT,
    DICTIONARY_PROMPT_DEFAULT,
    MAIN_PROMPT_DEFAULT,
    combine_prompt_sections,
)
from services.provider_registry import (
    LLMProviderId,
    STTProviderId,
    get_llm_provider_labels,
    get_stt_provider_labels,
)

if TYPE_CHECKING:
    from pipecat.pipeline.llm_switcher import LLMSwitcher
    from pipecat.pipeline.service_switcher import ServiceSwitcher
    from pipecat.services.ai_services import STTService
    from pipecat.services.llm_service import LLMService

    from config.settings import Settings
    from processors.llm_cleanup import TranscriptionToLLMConverter
    from processors.transcription_buffer import TranscriptionBufferProcessor

# Create router for config endpoints
config_router = APIRouter()

# Shared state - will be set by main server
_llm_converter: TranscriptionToLLMConverter | None = None
_transcription_buffer: TranscriptionBufferProcessor | None = None
_stt_switcher: ServiceSwitcher | None = None
_llm_switcher: LLMSwitcher | None = None
_stt_services: dict[STTProviderId, STTService] | None = None
_llm_services: dict[LLMProviderId, LLMService] | None = None
_settings: Settings | None = None

# Track current active providers
_current_stt_provider: STTProviderId | None = None
_current_llm_provider: LLMProviderId | None = None

# Event to signal when pipeline has started (received StartFrame)
_pipeline_started_event: asyncio.Event = asyncio.Event()


def set_pipeline_started() -> None:
    """Signal that the pipeline has started (received StartFrame)."""
    _pipeline_started_event.set()
    logger.info("Pipeline started event set")


def reset_pipeline_started() -> None:
    """Reset the pipeline started event (on disconnect)."""
    _pipeline_started_event.clear()
    logger.info("Pipeline started event cleared")


def set_llm_converter(converter: TranscriptionToLLMConverter) -> None:
    """Set the LLM converter reference for runtime prompt updates.

    Args:
        converter: The TranscriptionToLLMConverter instance from the pipeline.
    """
    global _llm_converter
    _llm_converter = converter


def set_transcription_buffer(buffer: TranscriptionBufferProcessor) -> None:
    """Set the transcription buffer reference for runtime timeout updates.

    Args:
        buffer: The TranscriptionBufferProcessor instance from the pipeline.
    """
    global _transcription_buffer
    _transcription_buffer = buffer


def set_service_switchers(
    stt_switcher: ServiceSwitcher,
    llm_switcher: LLMSwitcher,
    stt_services: dict[STTProviderId, Any],
    llm_services: dict[LLMProviderId, Any],
    settings: Settings,
) -> None:
    """Set the service switcher references for runtime provider switching.

    Args:
        stt_switcher: The STT ServiceSwitcher instance
        llm_switcher: The LLM Switcher instance
        stt_services: Dictionary mapping STT provider IDs to their services
        llm_services: Dictionary mapping LLM provider IDs to their services
        settings: Application settings
    """
    global _stt_switcher, _llm_switcher, _stt_services, _llm_services, _settings
    global _current_stt_provider, _current_llm_provider

    _stt_switcher = stt_switcher
    _llm_switcher = llm_switcher
    _stt_services = stt_services
    _llm_services = llm_services
    _settings = settings

    # Set initial active providers to first available
    if stt_services:
        _current_stt_provider = next(iter(stt_services.keys()))

    if llm_services:
        _current_llm_provider = next(iter(llm_services.keys()))


class PromptSectionData(BaseModel):
    """Data for a single prompt section."""

    enabled: bool
    content: str | None


class PromptSectionsData(BaseModel):
    """All prompt sections."""

    main: PromptSectionData
    advanced: PromptSectionData
    dictionary: PromptSectionData


class PromptSectionsUpdate(BaseModel):
    """Request body for updating prompt sections."""

    sections: PromptSectionsData


class DefaultSectionsResponse(BaseModel):
    """Response with default prompts for each section."""

    main: str
    advanced: str
    dictionary: str


class SetPromptResponse(BaseModel):
    """Response for setting the prompt."""

    success: bool
    error: str | None = None


@config_router.get("/api/prompt/sections/default", response_model=DefaultSectionsResponse)
async def get_default_sections() -> DefaultSectionsResponse:
    """Get default prompts for each section."""
    return DefaultSectionsResponse(
        main=MAIN_PROMPT_DEFAULT,
        advanced=ADVANCED_PROMPT_DEFAULT,
        dictionary=DICTIONARY_PROMPT_DEFAULT,
    )


@config_router.post("/api/prompt/sections", response_model=SetPromptResponse)
async def set_prompt_sections(data: PromptSectionsUpdate) -> SetPromptResponse:
    """Update prompt sections and combine them into the active prompt.

    Args:
        data: The prompt sections update request.
    """
    if _llm_converter:
        combined = combine_prompt_sections(
            main_enabled=data.sections.main.enabled,
            main_content=data.sections.main.content,
            advanced_enabled=data.sections.advanced.enabled,
            advanced_content=data.sections.advanced.content,
            dictionary_enabled=data.sections.dictionary.enabled,
            dictionary_content=data.sections.dictionary.content,
        )
        _llm_converter.set_custom_prompt(combined if combined else None)
        return SetPromptResponse(success=True)
    return SetPromptResponse(success=False, error="LLM converter not initialized")


# Provider Management Models


class ProviderInfo(BaseModel):
    """Information about a provider."""

    value: str
    label: str


class AvailableProvidersResponse(BaseModel):
    """Response listing available providers."""

    stt: list[ProviderInfo]
    llm: list[ProviderInfo]


class CurrentProvidersResponse(BaseModel):
    """Response for current active providers."""

    stt: str | None
    llm: str | None


class SwitchSTTProviderRequest(BaseModel):
    """Request to switch STT provider."""

    provider: STTProviderId


class SwitchSTTProviderResponse(BaseModel):
    """Response for STT provider switch."""

    success: bool
    provider: STTProviderId | None = None
    error: str | None = None


class SwitchLLMProviderRequest(BaseModel):
    """Request to switch LLM provider."""

    provider: LLMProviderId


class SwitchLLMProviderResponse(BaseModel):
    """Response for LLM provider switch."""

    success: bool
    provider: LLMProviderId | None = None
    error: str | None = None


# Provider Endpoints


@config_router.get("/api/providers/available", response_model=AvailableProvidersResponse)
async def get_available_providers() -> AvailableProvidersResponse:
    """Get list of available STT and LLM providers (those with API keys configured)."""
    stt_providers = []
    llm_providers = []
    stt_labels = get_stt_provider_labels()
    llm_labels = get_llm_provider_labels()

    if _stt_services:
        stt_providers = [
            ProviderInfo(
                value=provider_id.value, label=stt_labels.get(provider_id, provider_id.value)
            )
            for provider_id in _stt_services
        ]

    if _llm_services:
        llm_providers = [
            ProviderInfo(
                value=provider_id.value, label=llm_labels.get(provider_id, provider_id.value)
            )
            for provider_id in _llm_services
        ]

    return AvailableProvidersResponse(stt=stt_providers, llm=llm_providers)


@config_router.get("/api/providers/current", response_model=CurrentProvidersResponse)
async def get_current_providers() -> CurrentProvidersResponse:
    """Get currently active STT and LLM providers."""
    return CurrentProvidersResponse(
        stt=_current_stt_provider.value if _current_stt_provider else None,
        llm=_current_llm_provider.value if _current_llm_provider else None,
    )


@config_router.post("/api/providers/stt", response_model=SwitchSTTProviderResponse)
async def switch_stt_provider(data: SwitchSTTProviderRequest) -> SwitchSTTProviderResponse:
    """Switch to a different STT provider.

    Args:
        data: The provider to switch to.
    """
    global _current_stt_provider

    if not _stt_switcher or not _stt_services:
        return SwitchSTTProviderResponse(success=False, error="STT switcher not initialized")

    # Wait for pipeline to start before switching
    await _pipeline_started_event.wait()

    provider_id = data.provider

    if provider_id not in _stt_services:
        return SwitchSTTProviderResponse(
            success=False,
            error=f"Provider '{provider_id.value}' not available (no API key configured)",
        )

    service = _stt_services[provider_id]
    await _stt_switcher.process_frame(
        ManuallySwitchServiceFrame(service=service),
        FrameDirection.DOWNSTREAM,
    )
    _current_stt_provider = provider_id

    logger.success("switched_stt_provider", provider=provider_id.value)
    return SwitchSTTProviderResponse(success=True, provider=provider_id)


@config_router.post("/api/providers/llm", response_model=SwitchLLMProviderResponse)
async def switch_llm_provider(data: SwitchLLMProviderRequest) -> SwitchLLMProviderResponse:
    """Switch to a different LLM provider.

    Args:
        data: The provider to switch to.
    """
    global _current_llm_provider

    if not _llm_switcher or not _llm_services:
        return SwitchLLMProviderResponse(success=False, error="LLM switcher not initialized")

    # Wait for pipeline to start before switching
    await _pipeline_started_event.wait()

    provider_id = data.provider

    if provider_id not in _llm_services:
        return SwitchLLMProviderResponse(
            success=False,
            error=f"Provider '{provider_id.value}' not available (no API key configured)",
        )

    service = _llm_services[provider_id]
    await _llm_switcher.process_frame(
        ManuallySwitchServiceFrame(service=service),
        FrameDirection.DOWNSTREAM,
    )
    _current_llm_provider = provider_id

    logger.success("switched_llm_provider", provider=provider_id.value)
    return SwitchLLMProviderResponse(success=True, provider=provider_id)


# =============================================================================
# STT Timeout Configuration
# =============================================================================


class STTTimeoutRequest(BaseModel):
    """Request to set STT timeout."""

    timeout_seconds: float


class STTTimeoutResponse(BaseModel):
    """Response for STT timeout operations."""

    success: bool
    timeout_seconds: float | None = None
    error: str | None = None


@config_router.get("/api/config/stt-timeout", response_model=STTTimeoutResponse)
async def get_stt_timeout() -> STTTimeoutResponse:
    """Get the current STT transcription timeout."""
    if not _transcription_buffer:
        return STTTimeoutResponse(success=False, error="Transcription buffer not initialized")

    return STTTimeoutResponse(
        success=True,
        timeout_seconds=_transcription_buffer.get_transcription_timeout(),
    )


@config_router.post("/api/config/stt-timeout", response_model=STTTimeoutResponse)
async def set_stt_timeout(data: STTTimeoutRequest) -> STTTimeoutResponse:
    """Set the STT transcription timeout.

    Increase this value for STT providers that take longer to process.
    """
    if not _transcription_buffer:
        return STTTimeoutResponse(success=False, error="Transcription buffer not initialized")

    if data.timeout_seconds < 0.1 or data.timeout_seconds > 10.0:
        return STTTimeoutResponse(
            success=False,
            error="Timeout must be between 0.1 and 10.0 seconds",
        )

    _transcription_buffer.set_transcription_timeout(data.timeout_seconds)
    return STTTimeoutResponse(success=True, timeout_seconds=data.timeout_seconds)
