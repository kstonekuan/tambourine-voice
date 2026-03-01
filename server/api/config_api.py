"""HTTP API for configuration endpoints.

This module provides REST endpoints for:
- GET /api/prompt/sections/default - Get default prompt sections (static)
- PUT /api/config/prompts - Update prompt sections (per-client)
- PUT /api/config/stt-timeout - Update STT timeout (per-client)
- GET /api/providers - Get available providers (global)

Per-client endpoints use X-Client-UUID header to identify the client's pipeline.
Provider switching still uses RTVI since it requires frame injection into the pipeline.
"""

from __future__ import annotations

import json
import re
from typing import TYPE_CHECKING, Annotated, Any, Literal

from fastapi import APIRouter, Header, HTTPException, Request
from loguru import logger
from pipecat.processors.aggregators.llm_context import LLMContext
from pydantic import BaseModel, Field

from processors.llm import (
    ADVANCED_PROMPT_DEFAULT,
    DICTIONARY_PROMPT_DEFAULT,
    MAIN_PROMPT_DEFAULT,
)
from protocol.providers import (
    AutoProvider,
    KnownLLMProvider,
    OtherLLMProvider,
    parse_llm_provider_selection,
)
from services.provider_registry import (
    LLMProviderId,
    STTProviderId,
    get_llm_provider_labels,
    get_stt_provider_labels,
)
from services.providers import create_llm_service
from utils.rate_limiter import (
    RATE_LIMIT_CONFIG,
    RATE_LIMIT_PROVIDERS,
    RATE_LIMIT_RUNTIME_CONFIG,
    get_ip_only,
    limiter,
)

if TYPE_CHECKING:
    from processors.client_manager import ClientConnectionManager

config_router = APIRouter(prefix="/api", tags=["config"])

REQUIRED_MEMORY_SECTION_HEADERS = (
    "## Long-Term Signals",
    "## Ongoing Context",
    "## Active Threads",
    "## Recent Entities",
    "## Observed Recurring Phrases",
    "## Do Not Store",
    "## Metadata",
)

MARKDOWN_CODE_FENCE_PATTERN = re.compile(
    r"^```(?:markdown|md)?\s*(.*?)\s*```$",
    re.DOTALL | re.IGNORECASE,
)

MEMORY_SYNC_SYSTEM_PROMPT = """
You maintain a user memory file for dictation continuity.

Output requirements:
- Return only markdown, never wrap in code fences.
- Never include YAML frontmatter.
- Use exactly this section structure:
  # User Memory

  ## Long-Term Signals
  ## Ongoing Context
  ## Active Threads
  ## Recent Entities
  ## Observed Recurring Phrases
  ## Do Not Store
  ## Metadata

Content policy:
- Use only evidence from provided dictation history.
- Prefer concise bullets.
- If evidence is weak, keep placeholders rather than guessing.
- Do not store passwords, API keys, one-time codes, personal identifiers, or protected health information.
- In Metadata include:
  - Version: 1
  - Last Updated: <ISO-8601 UTC timestamp>
  - Sync cadence: every 3 completed sessions.
"""


# =============================================================================
# Pydantic models for prompt section configuration
# =============================================================================


class PromptModeAuto(BaseModel):
    """Auto mode: let the server optimize the prompt."""

    mode: Literal["auto"]


class PromptModeManual(BaseModel):
    """Manual mode: use user-provided custom content."""

    mode: Literal["manual"]
    content: str


PromptMode = Annotated[
    PromptModeAuto | PromptModeManual,
    Field(discriminator="mode"),
]


class PromptSection(BaseModel):
    """Configuration for a single prompt section.

    Two-layer structure:
    - enabled: Whether the section is active
    - mode: The prompt mode (auto or manual with content)
    """

    enabled: bool
    mode: PromptMode


class CleanupPromptSections(BaseModel):
    """Configuration for all cleanup prompt sections."""

    main: PromptSection
    advanced: PromptSection
    dictionary: PromptSection


class STTTimeoutRequest(BaseModel):
    """Request body for STT timeout update."""

    timeout_seconds: float


class LLMFormattingRequest(BaseModel):
    """Request body for LLM formatting configuration update.

    Simple boolean:
    - {"enabled": true}: Use LLM formatting
    - {"enabled": false}: Raw transcription (no LLM)
    """

    enabled: bool


class MemorySyncHistoryEntry(BaseModel):
    """Single dictation entry used as memory evidence."""

    timestamp: str
    text: str
    raw_text: str | None = None


class MemorySyncRequest(BaseModel):
    """Request body for server-backed memory synchronization."""

    llm_provider: str
    history_entries: list[MemorySyncHistoryEntry] = Field(min_length=1, max_length=50)
    existing_memory_markdown: str | None = None


class MemorySyncResponse(BaseModel):
    """Response body containing the full replacement memory markdown."""

    memory_markdown: str


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


class DefaultSectionsResponse(BaseModel):
    """Response with default prompts for each section."""

    main: str
    advanced: str
    dictionary: str


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


def strip_markdown_code_fence(raw_markdown_text: str) -> str:
    """Strip optional top-level markdown code fences returned by an LLM."""
    normalized_markdown_text = raw_markdown_text.strip()
    matched_code_fence = MARKDOWN_CODE_FENCE_PATTERN.match(normalized_markdown_text)
    if matched_code_fence is None:
        return normalized_markdown_text
    return matched_code_fence.group(1).strip()


def validate_generated_memory_markdown(memory_markdown: str) -> str:
    """Validate the generated memory markdown matches the locked schema."""
    normalized_markdown_text = strip_markdown_code_fence(memory_markdown)

    if normalized_markdown_text.startswith("---"):
        raise HTTPException(
            status_code=502,
            detail={
                "error": "Memory markdown must not start with YAML frontmatter",
                "code": "INVALID_MEMORY_MARKDOWN",
            },
        )

    if not normalized_markdown_text.startswith("# User Memory"):
        raise HTTPException(
            status_code=502,
            detail={
                "error": "Memory markdown must begin with '# User Memory'",
                "code": "INVALID_MEMORY_MARKDOWN",
            },
        )

    missing_sections = [
        required_section_header
        for required_section_header in REQUIRED_MEMORY_SECTION_HEADERS
        if required_section_header not in normalized_markdown_text
    ]
    if missing_sections:
        raise HTTPException(
            status_code=502,
            detail={
                "error": f"Memory markdown missing required sections: {', '.join(missing_sections)}",
                "code": "INVALID_MEMORY_MARKDOWN",
            },
        )

    return normalized_markdown_text


def resolve_memory_sync_provider_id(
    llm_provider_value: str,
    request: Request,
) -> LLMProviderId:
    """Resolve the requested provider value to a known available provider ID."""
    from main import AppServices

    services: AppServices = request.app.state.services
    parsed_provider_selection = parse_llm_provider_selection(llm_provider_value)

    if parsed_provider_selection is None:
        raise HTTPException(
            status_code=400,
            detail={"error": "Missing llm provider value", "code": "INVALID_LLM_PROVIDER"},
        )

    match parsed_provider_selection:
        case AutoProvider():
            configured_auto_llm_provider = services.settings.auto_llm_provider
            if configured_auto_llm_provider is None:
                available_llm_providers = services.available_llm_providers
                if not available_llm_providers:
                    raise HTTPException(
                        status_code=400,
                        detail={
                            "error": "No LLM providers are available on server",
                            "code": "INVALID_LLM_PROVIDER",
                        },
                    )
                resolved_provider_id = available_llm_providers[0]
                logger.info(
                    "AUTO_LLM_PROVIDER is unset; memory sync auto resolved "
                    f"to first available provider '{resolved_provider_id.value}'"
                )
            else:
                try:
                    resolved_provider_id = LLMProviderId(configured_auto_llm_provider)
                except ValueError as error:
                    raise HTTPException(
                        status_code=400,
                        detail={
                            "error": (
                                "Invalid auto LLM provider configured: "
                                f"{configured_auto_llm_provider}"
                            ),
                            "code": "INVALID_LLM_PROVIDER",
                        },
                    ) from error
        case KnownLLMProvider(provider_id=provider_id):
            resolved_provider_id = provider_id
        case OtherLLMProvider(provider_id=raw_provider_id):
            try:
                resolved_provider_id = LLMProviderId(raw_provider_id)
            except ValueError as error:
                raise HTTPException(
                    status_code=400,
                    detail={
                        "error": f"Unknown LLM provider: {raw_provider_id}",
                        "code": "INVALID_LLM_PROVIDER",
                    },
                ) from error

    if resolved_provider_id not in services.available_llm_providers:
        raise HTTPException(
            status_code=400,
            detail={
                "error": f"LLM provider '{resolved_provider_id.value}' is not available",
                "code": "INVALID_LLM_PROVIDER",
            },
        )

    return resolved_provider_id


def build_memory_sync_user_prompt(body: MemorySyncRequest) -> str:
    """Build the user prompt payload that the memory sync LLM consumes."""
    history_payload = [
        {
            "timestamp": history_entry.timestamp,
            "text": history_entry.text,
            "raw_text": history_entry.raw_text or "",
        }
        for history_entry in body.history_entries
    ]
    serialized_history_payload = json.dumps(history_payload, ensure_ascii=False)
    existing_memory_markdown = body.existing_memory_markdown or "(none)"

    return (
        "Current memory markdown (if any):\n"
        f"{existing_memory_markdown}\n\n"
        "Latest dictation history entries (JSON array):\n"
        f"{serialized_history_payload}\n\n"
        "Generate a full replacement markdown memory file that follows the required schema."
    )


# =============================================================================
# Endpoints
# =============================================================================


@config_router.get("/prompt/sections/default", response_model=DefaultSectionsResponse)
@limiter.limit(RATE_LIMIT_CONFIG, key_func=get_ip_only)
async def get_default_sections(request: Request) -> DefaultSectionsResponse:
    """Get default prompts for each section.

    Rate limited to prevent abuse, though this endpoint serves static data.
    """
    return DefaultSectionsResponse(
        main=MAIN_PROMPT_DEFAULT,
        advanced=ADVANCED_PROMPT_DEFAULT,
        dictionary=DICTIONARY_PROMPT_DEFAULT,
    )


@config_router.put(
    "/config/prompts",
    response_model=ConfigSuccessResponse,
    responses={
        404: {"model": ConfigErrorResponse, "description": "Client not connected"},
        422: {"model": ConfigErrorResponse, "description": "Validation failed"},
    },
)
@limiter.limit(RATE_LIMIT_RUNTIME_CONFIG, key_func=get_ip_only)
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

    def get_content(section: PromptSection) -> str | None:
        match section.mode:
            case PromptModeAuto():
                return None
            case PromptModeManual(content=content):
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


@config_router.put(
    "/config/llm-formatting",
    response_model=ConfigSuccessResponse,
    responses={
        404: {"model": ConfigErrorResponse, "description": "Client not connected"},
    },
)
@limiter.limit(RATE_LIMIT_RUNTIME_CONFIG, key_func=get_ip_only)
async def update_llm_formatting(
    body: LLMFormattingRequest,
    request: Request,
    x_client_uuid: Annotated[str, Header()],
) -> ConfigSuccessResponse:
    """Update the LLM formatting configuration for a connected client.

    Simple boolean:
    - {"enabled": true}: Use LLM formatting
    - {"enabled": false}: Raw transcription (no LLM)

    Args:
        body: Request body containing the enabled flag
        request: FastAPI request object
        x_client_uuid: Client UUID from X-Client-UUID header

    Returns:
        Success response with the updated setting

    Raises:
        HTTPException: 404 if client not connected
    """
    client_manager = get_client_manager(request)
    connection = client_manager.get_connection(x_client_uuid)

    if connection is None:
        raise HTTPException(
            status_code=404,
            detail={"error": "Client not connected", "code": "CLIENT_NOT_FOUND"},
        )

    if connection.llm_gate is None:
        raise HTTPException(
            status_code=404,
            detail={"error": "Pipeline not ready", "code": "PIPELINE_NOT_READY"},
        )

    connection.llm_gate.set_llm_formatting_enabled(body.enabled)

    logger.info(f"Set LLM formatting enabled={body.enabled} for client: {x_client_uuid}")
    return ConfigSuccessResponse(setting="llm-formatting", value=body.enabled)


@config_router.put(
    "/config/stt-timeout",
    response_model=ConfigSuccessResponse,
    responses={
        400: {"model": ConfigErrorResponse, "description": "Invalid timeout value"},
        404: {"model": ConfigErrorResponse, "description": "Client not connected"},
    },
)
@limiter.limit(RATE_LIMIT_RUNTIME_CONFIG, key_func=get_ip_only)
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


@config_router.post(
    "/config/memory-sync",
    response_model=MemorySyncResponse,
    responses={
        400: {"model": ConfigErrorResponse, "description": "Invalid request"},
        404: {"model": ConfigErrorResponse, "description": "Client not connected"},
        502: {"model": ConfigErrorResponse, "description": "LLM generation failed"},
    },
)
@limiter.limit(RATE_LIMIT_RUNTIME_CONFIG, key_func=get_ip_only)
async def sync_memory_markdown(
    body: MemorySyncRequest,
    request: Request,
    x_client_uuid: Annotated[str, Header()],
) -> MemorySyncResponse:
    """Generate full replacement memory markdown from history using an LLM."""
    client_manager = get_client_manager(request)
    connection = client_manager.get_connection(x_client_uuid)
    if connection is None:
        raise HTTPException(
            status_code=404,
            detail={"error": "Client not connected", "code": "CLIENT_NOT_FOUND"},
        )

    from main import AppServices

    services: AppServices = request.app.state.services
    provider_id = resolve_memory_sync_provider_id(body.llm_provider, request)
    llm_service = create_llm_service(provider_id, services.settings)

    llm_context = LLMContext(
        messages=[
            {"role": "system", "content": MEMORY_SYNC_SYSTEM_PROMPT},
            {"role": "user", "content": build_memory_sync_user_prompt(body)},
        ]
    )
    try:
        generated_memory_markdown = await llm_service.run_inference(llm_context)
    except Exception as inference_error:
        logger.warning(
            f"Memory sync inference failed for client {x_client_uuid}: {inference_error}"
        )
        raise HTTPException(
            status_code=502,
            detail={
                "error": "LLM memory generation failed",
                "code": "LLM_GENERATION_FAILED",
            },
        ) from inference_error

    if not generated_memory_markdown or not generated_memory_markdown.strip():
        raise HTTPException(
            status_code=502,
            detail={
                "error": "LLM returned empty memory markdown",
                "code": "EMPTY_MEMORY_MARKDOWN",
            },
        )

    validated_memory_markdown = validate_generated_memory_markdown(generated_memory_markdown)
    logger.info(f"Generated memory markdown for client {x_client_uuid}")
    return MemorySyncResponse(memory_markdown=validated_memory_markdown)


@config_router.get(
    "/providers",
    response_model=AvailableProvidersResponse,
)
@limiter.limit(RATE_LIMIT_PROVIDERS, key_func=get_ip_only)
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
            local_provider_ids={STTProviderId.WHISPER, STTProviderId.WHISPER_MLX},
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
