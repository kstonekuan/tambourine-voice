"""HTTP API for runtime pipeline configuration.

This module provides REST endpoints for configuring pipeline settings via HTTP,
using X-Client-UUID header to identify the client's pipeline:
- PUT /api/config/prompts - Update prompt sections
- PUT /api/config/stt-timeout - Update STT timeout
- GET /api/providers - Get available providers (global, no UUID required)

These endpoints replace the RTVI data channel configuration for state-only updates.
Provider switching still uses RTVI since it requires frame injection into the pipeline.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Annotated, Any, Literal

from fastapi import APIRouter, Header, HTTPException, Request
from loguru import logger
from pydantic import BaseModel, Field

from services.provider_registry import (
    LLMProviderId,
    STTProviderId,
    get_llm_provider_labels,
    get_stt_provider_labels,
)

if TYPE_CHECKING:
    from processors.client_manager import ClientConnectionManager

runtime_config_router = APIRouter(prefix="/api", tags=["runtime-config"])


# =============================================================================
# Pydantic models for prompt section configuration
# =============================================================================


class PromptSectionAuto(BaseModel):
    """Auto mode: use server's built-in default prompt."""

    enabled: bool
    mode: Literal["auto"]


class PromptSectionManual(BaseModel):
    """Manual mode: use user-provided content."""

    enabled: bool
    mode: Literal["manual"]
    content: str


PromptSection = Annotated[
    PromptSectionAuto | PromptSectionManual,
    Field(discriminator="mode"),
]


class CleanupPromptSections(BaseModel):
    """Configuration for all cleanup prompt sections."""

    main: PromptSection
    advanced: PromptSection
    dictionary: PromptSection


class STTTimeoutRequest(BaseModel):
    """Request body for STT timeout update."""

    timeout_seconds: float


class ConfigSuccessResponse(BaseModel):
    """Response for successful configuration update."""

    success: Literal[True] = True
    setting: str
    value: Any = None


class ConfigErrorResponse(BaseModel):
    """Response for configuration errors."""

    error: str
    code: str
    details: list[Any] | None = None


class ProviderInfo(BaseModel):
    """Information about an available provider."""

    value: str
    label: str
    is_local: bool
    model: str | None = None


class AvailableProvidersResponse(BaseModel):
    """Response containing available STT and LLM providers."""

    stt: list[ProviderInfo]
    llm: list[ProviderInfo]


# =============================================================================
# Helper functions
# =============================================================================


def get_client_manager(request: Request) -> ClientConnectionManager:
    """Get the client manager from app state."""
    from main import AppServices

    services: AppServices = request.app.state.services
    return services.client_manager


def build_provider_list(
    services: dict[Any, Any],
    labels: dict[Any, str],
    local_provider_ids: set[Any],
) -> list[ProviderInfo]:
    """Build a provider info list from services.

    Args:
        services: Dictionary mapping provider IDs to service instances
        labels: Dictionary mapping provider IDs to display labels
        local_provider_ids: Set of provider IDs that are local (not cloud)

    Returns:
        List of ProviderInfo objects
    """
    return [
        ProviderInfo(
            value=provider_id.value,
            label=labels.get(provider_id, provider_id.value),
            is_local=provider_id in local_provider_ids,
            model=getattr(service, "model_name", None),
        )
        for provider_id, service in services.items()
    ]


# =============================================================================
# Endpoints
# =============================================================================


@runtime_config_router.put(
    "/config/prompts",
    response_model=ConfigSuccessResponse,
    responses={
        404: {"model": ConfigErrorResponse, "description": "Client not connected"},
        422: {"model": ConfigErrorResponse, "description": "Validation failed"},
    },
)
async def update_prompt_sections(
    sections: CleanupPromptSections,
    request: Request,
    x_client_uuid: Annotated[str, Header()],
) -> ConfigSuccessResponse:
    """Update the LLM formatting prompt sections for a connected client.

    Args:
        sections: The new prompt sections configuration
        request: FastAPI request object
        x_client_uuid: Client UUID from X-Client-UUID header

    Returns:
        Success response with the updated setting name

    Raises:
        HTTPException: 404 if client not connected, 422 if validation fails
    """
    client_manager = get_client_manager(request)
    connection = client_manager.get_connection(x_client_uuid)

    if connection is None:
        raise HTTPException(
            status_code=404,
            detail={"error": "Client not connected", "code": "CLIENT_NOT_FOUND"},
        )

    if connection.context_manager is None:
        raise HTTPException(
            status_code=404,
            detail={"error": "Pipeline not ready", "code": "PIPELINE_NOT_READY"},
        )

    # Extract content from discriminated union (None for auto mode)
    def get_content(section: PromptSectionAuto | PromptSectionManual) -> str | None:
        match section:
            case PromptSectionAuto():
                return None
            case PromptSectionManual(content=content):
                return content

    connection.context_manager.set_prompt_sections(
        main_custom=get_content(sections.main),
        advanced_enabled=sections.advanced.enabled,
        advanced_custom=get_content(sections.advanced),
        dictionary_enabled=sections.dictionary.enabled,
        dictionary_custom=get_content(sections.dictionary),
    )

    logger.info(f"Updated prompt sections for client: {x_client_uuid}")
    return ConfigSuccessResponse(setting="prompt-sections", value="custom")


@runtime_config_router.put(
    "/config/stt-timeout",
    response_model=ConfigSuccessResponse,
    responses={
        400: {"model": ConfigErrorResponse, "description": "Invalid timeout value"},
        404: {"model": ConfigErrorResponse, "description": "Client not connected"},
    },
)
async def update_stt_timeout(
    body: STTTimeoutRequest,
    request: Request,
    x_client_uuid: Annotated[str, Header()],
) -> ConfigSuccessResponse:
    """Update the STT transcription timeout for a connected client.

    Args:
        body: Request body containing the timeout value
        request: FastAPI request object
        x_client_uuid: Client UUID from X-Client-UUID header

    Returns:
        Success response with the updated timeout value

    Raises:
        HTTPException: 400 if timeout invalid, 404 if client not connected
    """
    client_manager = get_client_manager(request)
    connection = client_manager.get_connection(x_client_uuid)

    if connection is None:
        raise HTTPException(
            status_code=404,
            detail={"error": "Client not connected", "code": "CLIENT_NOT_FOUND"},
        )

    if connection.turn_controller is None:
        raise HTTPException(
            status_code=404,
            detail={"error": "Pipeline not ready", "code": "PIPELINE_NOT_READY"},
        )

    if body.timeout_seconds < 0.1 or body.timeout_seconds > 10.0:
        raise HTTPException(
            status_code=400,
            detail={
                "error": "Timeout must be between 0.1 and 10.0 seconds",
                "code": "INVALID_TIMEOUT",
            },
        )

    connection.turn_controller.set_transcription_timeout(body.timeout_seconds)

    logger.info(f"Set STT timeout to {body.timeout_seconds}s for client: {x_client_uuid}")
    return ConfigSuccessResponse(setting="stt-timeout", value=body.timeout_seconds)


@runtime_config_router.get(
    "/providers",
    response_model=AvailableProvidersResponse,
)
async def get_available_providers(request: Request) -> AvailableProvidersResponse:
    """Get available STT and LLM providers.

    This endpoint is global (not per-client) because available providers are
    determined by server configuration (API keys), not per-client state.
    All clients see the same available providers.

    To get model information, we need an active connection. If no connections
    exist, returns providers without model info.

    Args:
        request: FastAPI request object

    Returns:
        Response containing lists of available STT and LLM providers
    """
    client_manager = get_client_manager(request)

    # Try to get services from any active connection for model info
    # All connections have the same available providers (based on API keys)
    stt_services: dict[STTProviderId, Any] | None = None
    llm_services: dict[LLMProviderId, Any] | None = None

    # Get first active connection's services
    for uuid in list(client_manager._connections.keys()):
        conn = client_manager.get_connection(uuid)
        if conn and conn.stt_services and conn.llm_services:
            stt_services = conn.stt_services
            llm_services = conn.llm_services
            break

    if stt_services and llm_services:
        stt_providers = build_provider_list(
            services=stt_services,
            labels=get_stt_provider_labels(),
            local_provider_ids={STTProviderId.WHISPER},
        )
        llm_providers = build_provider_list(
            services=llm_services,
            labels=get_llm_provider_labels(),
            local_provider_ids={LLMProviderId.OLLAMA},
        )
    else:
        # No active connections - return empty lists
        # Client should retry after connection is established
        stt_providers = []
        llm_providers = []

    return AvailableProvidersResponse(stt=stt_providers, llm=llm_providers)
