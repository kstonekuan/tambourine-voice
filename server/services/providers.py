"""Provider factory functions for STT and LLM services.

This module provides factory functions that use the provider registry to
create service instances with direct class instantiation (no importlib).
"""

from typing import TYPE_CHECKING

from loguru import logger
from pipecat.services.llm_service import LLMService
from pipecat.services.stt_service import STTService

from services.provider_registry import (
    LLM_PROVIDERS,
    STT_PROVIDERS,
    LLMProviderConfig,
    LLMProviderId,
    STTProviderConfig,
    STTProviderId,
    get_llm_provider_config,
    get_llm_provider_labels,
    get_stt_provider_config,
    get_stt_provider_labels,
)

if TYPE_CHECKING:
    from config.settings import Settings

__all__ = [
    "LLMProviderId",
    "STTProviderId",
    "create_all_available_llm_services",
    "create_all_available_stt_services",
    "create_llm_service",
    "create_stt_service",
    "get_llm_provider_labels",
    "get_stt_provider_labels",
]


def _create_stt_service_from_config(
    config: STTProviderConfig,
    settings: "Settings",
) -> STTService:
    """Create an STT service instance from a provider config.

    Args:
        config: The provider configuration with direct class reference
        settings: Application settings containing API keys

    Returns:
        Configured STT service instance

    Raises:
        ValueError: If required credentials are not configured
    """
    if not config.credential_mapper.is_available(settings):
        missing = [
            field
            for field in config.credential_mapper.get_required_fields()
            if not getattr(settings, field, None)
        ]
        raise ValueError(f"{config.display_name} requires: {', '.join(missing)}")

    # Build kwargs from credential mapper + default kwargs
    kwargs = config.credential_mapper.map_credentials(settings)
    kwargs.update(config.default_kwargs)

    logger.info(f"Creating STT service: {config.provider_id.value}")

    # Direct instantiation - service_class is type-checked at import time
    return config.service_class(**kwargs)


def _create_llm_service_from_config(
    config: LLMProviderConfig,
    settings: "Settings",
) -> LLMService:
    """Create an LLM service instance from a provider config.

    Args:
        config: The provider configuration with direct class reference
        settings: Application settings containing API keys

    Returns:
        Configured LLM service instance

    Raises:
        ValueError: If required credentials are not configured
    """
    if not config.credential_mapper.is_available(settings):
        missing = [
            field
            for field in config.credential_mapper.get_required_fields()
            if not getattr(settings, field, None)
        ]
        raise ValueError(f"{config.display_name} requires: {', '.join(missing)}")

    # Build kwargs from credential mapper + default kwargs
    kwargs = config.credential_mapper.map_credentials(settings)
    kwargs.update(config.default_kwargs)

    logger.info(f"Creating LLM service: {config.provider_id.value}")

    # Direct instantiation - service_class is type-checked at import time
    return config.service_class(**kwargs)


def create_stt_service(provider_id: STTProviderId, settings: "Settings") -> STTService:
    """Create an STT service instance for the given provider.

    Args:
        provider_id: The STT provider ID enum
        settings: Application settings containing API keys

    Returns:
        Configured STT service instance

    Raises:
        ValueError: If the provider is unknown or API key is not configured
    """
    config = get_stt_provider_config(provider_id)
    if not config:
        raise ValueError(f"Unknown STT provider: {provider_id}")
    return _create_stt_service_from_config(config, settings)


def create_llm_service(provider_id: LLMProviderId, settings: "Settings") -> LLMService:
    """Create an LLM service instance for the given provider.

    Args:
        provider_id: The LLM provider ID enum
        settings: Application settings containing API keys

    Returns:
        Configured LLM service instance

    Raises:
        ValueError: If the provider is unknown or API key is not configured
    """
    config = get_llm_provider_config(provider_id)
    if not config:
        raise ValueError(f"Unknown LLM provider: {provider_id}")
    return _create_llm_service_from_config(config, settings)


def get_available_stt_providers(settings: "Settings") -> list[STTProviderId]:
    """Get list of STT providers that have API keys configured.

    Args:
        settings: Application settings

    Returns:
        List of available STT provider IDs
    """
    return [
        config.provider_id
        for config in STT_PROVIDERS.values()
        if config.credential_mapper.is_available(settings)
    ]


def get_available_llm_providers(settings: "Settings") -> list[LLMProviderId]:
    """Get list of LLM providers that have API keys configured.

    Args:
        settings: Application settings

    Returns:
        List of available LLM provider IDs
    """
    return [
        config.provider_id
        for config in LLM_PROVIDERS.values()
        if config.credential_mapper.is_available(settings)
    ]


def create_all_available_stt_services(
    settings: "Settings",
    available_providers: list[STTProviderId],
) -> dict[STTProviderId, STTService]:
    """Create STT service instances for all available providers.

    Args:
        settings: Application settings
        available_providers: Pre-computed list of available STT provider IDs

    Returns:
        Dictionary mapping provider ID to service instance
    """
    services: dict[STTProviderId, STTService] = {}

    for provider_id in available_providers:
        try:
            services[provider_id] = create_stt_service(provider_id, settings)
        except Exception as e:
            logger.warning(f"Failed to create STT service '{provider_id.value}': {e}")

    return services


def create_all_available_llm_services(
    settings: "Settings",
    available_providers: list[LLMProviderId],
) -> dict[LLMProviderId, LLMService]:
    """Create LLM service instances for all available providers.

    Args:
        settings: Application settings
        available_providers: Pre-computed list of available LLM provider IDs

    Returns:
        Dictionary mapping provider ID to service instance
    """
    services: dict[LLMProviderId, LLMService] = {}

    for provider_id in available_providers:
        try:
            services[provider_id] = create_llm_service(provider_id, settings)
        except Exception as e:
            logger.warning(f"Failed to create LLM service '{provider_id.value}': {e}")

    return services
