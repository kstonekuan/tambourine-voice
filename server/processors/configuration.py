"""Configuration handler for runtime provider switching via RTVI client messages.

This module provides configuration handling for switching STT and LLM providers
at runtime. Provider switching requires ManuallySwitchServiceFrame injection
into the pipeline, which is why it uses RTVI data channel rather than HTTP API.

State-only configuration (prompts, timeouts) has been moved to HTTP API endpoints
in api/config_api.py.
"""

from __future__ import annotations

from types.messages import ClientMessageType, ServerMessageType
from types.providers import (
    AutoProvider,
    KnownLLMProvider,
    KnownSTTProvider,
    LLMProviderSelection,
    OtherLLMProvider,
    OtherSTTProvider,
    STTProviderSelection,
    parse_llm_provider_selection,
    parse_stt_provider_selection,
)
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
        match msg_type:
            case ClientMessageType.SET_STT_PROVIDER:
                logger.debug(f"Received config message: type={msg_type}")
                selection = parse_stt_provider_selection(data.get("provider"))
                await self._switch_stt_provider(selection)
                return True
            case ClientMessageType.SET_LLM_PROVIDER:
                logger.debug(f"Received config message: type={msg_type}")
                selection = parse_llm_provider_selection(data.get("provider"))
                await self._switch_llm_provider(selection)
                return True
            case _:
                return False

    async def _switch_stt_provider(self, selection: STTProviderSelection | None) -> None:
        """Switch to a different STT provider.

        Args:
            selection: The parsed provider selection (auto, known, or other)
        """
        setting_name = "stt-provider"

        if selection is None:
            await self._send_config_error(setting_name, "Provider value is required")
            return

        match selection:
            case AutoProvider():
                if self._settings.auto_stt_provider is None:
                    logger.warning("No auto STT provider configured, no-op")
                    await self._send_config_success(setting_name, "auto")
                    return
                try:
                    provider_id = STTProviderId(self._settings.auto_stt_provider)
                except ValueError:
                    await self._send_config_error(
                        setting_name,
                        f"Invalid auto STT provider configured: {self._settings.auto_stt_provider}",
                    )
                    return
                logger.info(f"Auto mode for STT resolved to: {provider_id.value}")
            case KnownSTTProvider(provider_id=provider_id):
                pass  # Use directly
            case OtherSTTProvider(provider_id=raw_id):
                try:
                    provider_id = STTProviderId(raw_id)
                except ValueError:
                    await self._send_config_error(setting_name, f"Unknown provider: {raw_id}")
                    return

        if provider_id not in self._stt_services:
            await self._send_config_error(
                setting_name,
                f"Provider '{provider_id.value}' not available (no API key configured)",
            )
            return

        service = self._stt_services[provider_id]
        await self._stt_switcher.process_frame(
            ManuallySwitchServiceFrame(service=service),
            FrameDirection.DOWNSTREAM,
        )

        logger.success(f"Switched STT provider to: {provider_id.value}")
        await self._send_config_success(setting_name, provider_id.value)

    async def _switch_llm_provider(self, selection: LLMProviderSelection | None) -> None:
        """Switch to a different LLM provider.

        Args:
            selection: The parsed provider selection (auto, known, or other)
        """
        setting_name = "llm-provider"

        if selection is None:
            await self._send_config_error(setting_name, "Provider value is required")
            return

        match selection:
            case AutoProvider():
                if self._settings.auto_llm_provider is None:
                    logger.warning("No auto LLM provider configured, no-op")
                    await self._send_config_success(setting_name, "auto")
                    return
                try:
                    provider_id = LLMProviderId(self._settings.auto_llm_provider)
                except ValueError:
                    await self._send_config_error(
                        setting_name,
                        f"Invalid auto LLM provider configured: {self._settings.auto_llm_provider}",
                    )
                    return
                logger.info(f"Auto mode for LLM resolved to: {provider_id.value}")
            case KnownLLMProvider(provider_id=provider_id):
                pass  # Use directly
            case OtherLLMProvider(provider_id=raw_id):
                try:
                    provider_id = LLMProviderId(raw_id)
                except ValueError:
                    await self._send_config_error(setting_name, f"Unknown provider: {raw_id}")
                    return

        if provider_id not in self._llm_services:
            await self._send_config_error(
                setting_name,
                f"Provider '{provider_id.value}' not available (no API key configured)",
            )
            return

        service = self._llm_services[provider_id]
        await self._llm_switcher.process_frame(
            ManuallySwitchServiceFrame(service=service),
            FrameDirection.DOWNSTREAM,
        )

        logger.success(f"Switched LLM provider to: {provider_id.value}")
        await self._send_config_success(setting_name, provider_id.value)

    async def _send_config_success(self, setting: str, value: Any) -> None:
        """Send a configuration success message to the client."""
        frame = RTVIServerMessageFrame(
            data={
                "type": ServerMessageType.CONFIG_UPDATED,
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
                "type": ServerMessageType.CONFIG_ERROR,
                "setting": setting,
                "error": error,
            }
        )
        await self._rtvi.push_frame(frame)
        logger.warning(f"Config error for {setting}: {error}")
