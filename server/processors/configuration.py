"""Configuration handler for runtime provider switching via RTVI client messages.

This module provides configuration handling for switching STT and LLM providers
at runtime. Provider switching requires ManuallySwitchServiceFrame injection
into the pipeline, which is why it uses RTVI data channel rather than HTTP API.

State-only configuration (prompts, timeouts) has been moved to HTTP API endpoints
in api/config_api.py.
"""

from __future__ import annotations

from enum import StrEnum
from typing import TYPE_CHECKING, Any

from loguru import logger
from pipecat.frames.frames import ManuallySwitchServiceFrame
from pipecat.processors.frame_processor import FrameDirection
from pipecat.processors.frameworks.rtvi import RTVIProcessor, RTVIServerMessageFrame

from services.provider_registry import LLMProviderId, STTProviderId

if TYPE_CHECKING:
    from pipecat.pipeline.llm_switcher import LLMSwitcher
    from pipecat.pipeline.service_switcher import ServiceSwitcher
    from pipecat.services.ai_services import STTService
    from pipecat.services.llm_service import LLMService

    from config.settings import Settings


class ConfigurationHandler:
    """Handles provider switching via RTVI client messages.

    This handler is registered with RTVIProcessor's on_client_message event
    to process provider switching messages:
    - set-stt-provider: Switch STT service
    - set-llm-provider: Switch LLM service

    Provider switching requires ManuallySwitchServiceFrame to be injected into
    the pipeline, which is why these remain on the RTVI data channel rather than
    moving to HTTP API.

    State-only configuration (prompts, timeouts, available providers) has been
    moved to HTTP API endpoints for simpler client integration.
    """

    def __init__(
        self,
        rtvi_processor: RTVIProcessor,
        stt_switcher: ServiceSwitcher,
        llm_switcher: LLMSwitcher,
        stt_services: dict[STTProviderId, STTService],
        llm_services: dict[LLMProviderId, LLMService],
        settings: Settings,
    ) -> None:
        """Initialize the configuration handler.

        Args:
            rtvi_processor: The RTVIProcessor to send responses through
            stt_switcher: ServiceSwitcher for STT services
            llm_switcher: LLMSwitcher for LLM services
            stt_services: Dictionary mapping STT provider IDs to services
            llm_services: Dictionary mapping LLM provider IDs to services
            settings: Application settings for auto provider configuration
        """
        self._rtvi = rtvi_processor
        self._stt_switcher = stt_switcher
        self._llm_switcher = llm_switcher
        self._stt_services = stt_services
        self._llm_services = llm_services
        self._settings = settings

    async def handle_client_message(self, msg_type: str, data: dict[str, Any]) -> bool:
        """Handle a client message from RTVIProcessor.

        Args:
            msg_type: The message type (e.g., "set-stt-provider")
            data: The message data payload

        Returns:
            True if the message was handled as a config message
        """
        handlers: dict[str, Any] = {
            "set-stt-provider": lambda: self._switch_provider(
                provider_value=data.get("provider"),
                setting_name="stt-provider",
                provider_enum=STTProviderId,
                services=self._stt_services,
                switcher=self._stt_switcher,
            ),
            "set-llm-provider": lambda: self._switch_provider(
                provider_value=data.get("provider"),
                setting_name="llm-provider",
                provider_enum=LLMProviderId,
                services=self._llm_services,
                switcher=self._llm_switcher,
            ),
        }

        handler = handlers.get(msg_type)
        if handler is None:
            return False

        logger.debug(f"Received config message: type={msg_type}")
        await handler()
        return True

    async def _switch_provider(
        self,
        provider_value: str | None,
        setting_name: str,
        provider_enum: type[StrEnum],
        services: dict[Any, Any],
        switcher: ServiceSwitcher | LLMSwitcher,
    ) -> None:
        """Switch to a different provider (generic for STT/LLM).

        Args:
            provider_value: The provider ID string (e.g., "deepgram", "openai", "auto")
            setting_name: The setting name for responses (e.g., "stt-provider")
            provider_enum: The enum class to validate against
            services: Dictionary mapping provider IDs to services
            switcher: The service switcher to use
        """
        if not provider_value:
            await self._send_config_error(setting_name, "Provider value is required")
            return

        # Handle "auto" provider - resolve to configured auto provider or use pipecat default
        if provider_value == "auto":
            if setting_name == "stt-provider":
                auto_provider = self._settings.auto_stt_provider
            else:
                auto_provider = self._settings.auto_llm_provider

            if auto_provider is None:
                # No auto provider configured - log warning and no-op
                logger.warning(f"No auto provider configured for {setting_name}, no-op")
                await self._send_config_success(setting_name, "auto")
                return

            # Resolve "auto" to the configured provider
            provider_value = auto_provider
            logger.info(f"Auto mode for {setting_name} resolved to: {auto_provider}")

        try:
            provider_id = provider_enum(provider_value)
        except ValueError:
            await self._send_config_error(setting_name, f"Unknown provider: {provider_value}")
            return

        if provider_id not in services:
            await self._send_config_error(
                setting_name,
                f"Provider '{provider_value}' not available (no API key configured)",
            )
            return

        service = services[provider_id]
        await switcher.process_frame(
            ManuallySwitchServiceFrame(service=service),
            FrameDirection.DOWNSTREAM,
        )

        logger.success(f"Switched {setting_name} to: {provider_value}")
        await self._send_config_success(setting_name, provider_value)

    async def _send_config_success(self, setting: str, value: Any) -> None:
        """Send a configuration success message to the client."""
        frame = RTVIServerMessageFrame(
            data={
                "type": "config-updated",
                "setting": setting,
                "value": value,
                "success": True,
            }
        )
        await self._rtvi.push_frame(frame)

    async def _send_config_error(self, setting: str, error: str) -> None:
        """Send a configuration error message to the client."""
        frame = RTVIServerMessageFrame(
            data={
                "type": "config-error",
                "setting": setting,
                "error": error,
            }
        )
        await self._rtvi.push_frame(frame)
        logger.warning(f"Config error for {setting}: {error}")
