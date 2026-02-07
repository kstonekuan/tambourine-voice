"""Configuration handler for runtime provider switching via RTVI client messages.

This module provides configuration handling for switching STT and LLM providers
at runtime. Provider switching requires ManuallySwitchServiceFrame injection
into the pipeline, which is why it uses RTVI data channel rather than HTTP API.

State-only configuration (prompts, timeouts) has been moved to HTTP API endpoints
in api/config_api.py.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from loguru import logger
from pipecat.frames.frames import ManuallySwitchServiceFrame
from pipecat.processors.frame_processor import FrameDirection
from pipecat.processors.frameworks.rtvi import RTVIProcessor, RTVIServerMessageFrame

from protocol.messages import (
    ConfigErrorMessage,
    ConfigMessage,
    ConfigUpdatedMessage,
    SetLLMProviderMessage,
    SetSTTProviderMessage,
    SettingName,
)
from protocol.providers import (
    AutoProvider,
    KnownLLMProvider,
    KnownSTTProvider,
    LLMProviderId,
    LLMProviderSelection,
    OtherLLMProvider,
    OtherSTTProvider,
    STTProviderId,
    STTProviderSelection,
)

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

    async def handle_config_message(self, message: ConfigMessage) -> None:
        """Handle a typed configuration message.

        Args:
            message: The parsed configuration message (SetSTTProviderMessage or SetLLMProviderMessage)
        """
        match message:
            case SetSTTProviderMessage(data=data):
                logger.debug(f"Received config message: type={message.type}")
                await self._switch_stt_provider(data.provider)
            case SetLLMProviderMessage(data=data):
                logger.debug(f"Received config message: type={message.type}")
                await self._switch_llm_provider(data.provider)

    async def _switch_stt_provider(self, selection: STTProviderSelection) -> None:
        """Switch to a different STT provider.

        Args:
            selection: The provider selection (auto, known, or other)
        """
        setting = SettingName.STT_PROVIDER

        match selection:
            case AutoProvider():
                if self._settings.auto_stt_provider is None:
                    logger.warning("No auto STT provider configured, no-op")
                    await self._send_config_success(setting, selection)
                    return
                try:
                    provider_id = STTProviderId(self._settings.auto_stt_provider)
                except ValueError:
                    await self._send_config_error(
                        setting,
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
                    await self._send_config_error(setting, f"Unknown provider: {raw_id}")
                    return

        if provider_id not in self._stt_services:
            await self._send_config_error(
                setting,
                f"Provider '{provider_id.value}' not available (no API key configured)",
            )
            return

        service = self._stt_services[provider_id]
        await self._stt_switcher.process_frame(
            ManuallySwitchServiceFrame(service=service),
            FrameDirection.DOWNSTREAM,
        )

        logger.success(f"Switched STT provider to: {provider_id.value}")
        # Echo back the original selection - client sent it, server validated it works
        await self._send_config_success(setting, selection)

    async def _switch_llm_provider(self, selection: LLMProviderSelection) -> None:
        """Switch to a different LLM provider.

        Args:
            selection: The provider selection (auto, known, or other)
        """
        setting = SettingName.LLM_PROVIDER

        match selection:
            case AutoProvider():
                if self._settings.auto_llm_provider is None:
                    logger.warning("No auto LLM provider configured, no-op")
                    await self._send_config_success(setting, selection)
                    return
                try:
                    provider_id = LLMProviderId(self._settings.auto_llm_provider)
                except ValueError:
                    await self._send_config_error(
                        setting,
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
                    await self._send_config_error(setting, f"Unknown provider: {raw_id}")
                    return

        if provider_id not in self._llm_services:
            await self._send_config_error(
                setting,
                f"Provider '{provider_id.value}' not available (no API key configured)",
            )
            return

        service = self._llm_services[provider_id]
        await self._llm_switcher.process_frame(
            ManuallySwitchServiceFrame(service=service),
            FrameDirection.DOWNSTREAM,
        )

        logger.success(f"Switched LLM provider to: {provider_id.value}")
        # Echo back the original selection - client sent it, server validated it works
        await self._send_config_success(setting, selection)

    async def _send_config_success(
        self, setting: SettingName, value: STTProviderSelection | LLMProviderSelection
    ) -> None:
        """Send a configuration success message to the client.

        The value is a selection type (AutoProvider or Known*Provider) that
        matches the format sent by the client, ensuring symmetric serialization.
        """
        message = ConfigUpdatedMessage(setting=setting, value=value)
        frame = RTVIServerMessageFrame(data=message.model_dump(by_alias=True))
        await self._rtvi.push_frame(frame)

    async def _send_config_error(self, setting: SettingName, error: str) -> None:
        """Send a configuration error message to the client."""
        message = ConfigErrorMessage(setting=setting, error=error)
        frame = RTVIServerMessageFrame(data=message.model_dump())
        await self._rtvi.push_frame(frame)
        logger.warning(f"Config error for {setting}: {error}")
