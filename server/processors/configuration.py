"""Configuration handler for runtime configuration via RTVI client messages.

This module provides configuration handling for the pipeline, called from
RTVIProcessor's on_client_message event handler.
"""

from __future__ import annotations

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

    from processors.llm import TranscriptionToLLMConverter
    from processors.transcription_buffer import TranscriptionBufferProcessor


class ConfigurationHandler:
    """Handles configuration messages from RTVI client messages.

    This handler is registered with RTVIProcessor's on_client_message event
    to process config messages like:
    - set-stt-provider: Switch STT service
    - set-llm-provider: Switch LLM service
    - set-prompt-sections: Update LLM prompt
    - set-stt-timeout: Update transcription timeout

    All configuration is scoped to this pipeline instance.
    """

    def __init__(
        self,
        rtvi_processor: RTVIProcessor,
        stt_switcher: ServiceSwitcher,
        llm_switcher: LLMSwitcher,
        llm_converter: TranscriptionToLLMConverter,
        transcription_buffer: TranscriptionBufferProcessor,
        stt_services: dict[STTProviderId, STTService],
        llm_services: dict[LLMProviderId, LLMService],
    ) -> None:
        """Initialize the configuration handler.

        Args:
            rtvi_processor: The RTVIProcessor to send responses through
            stt_switcher: ServiceSwitcher for STT services
            llm_switcher: LLMSwitcher for LLM services
            llm_converter: TranscriptionToLLMConverter for prompt configuration
            transcription_buffer: TranscriptionBufferProcessor for timeout configuration
            stt_services: Dictionary mapping STT provider IDs to services
            llm_services: Dictionary mapping LLM provider IDs to services
        """
        self._rtvi = rtvi_processor
        self._stt_switcher = stt_switcher
        self._llm_switcher = llm_switcher
        self._llm_converter = llm_converter
        self._transcription_buffer = transcription_buffer
        self._stt_services = stt_services
        self._llm_services = llm_services

        # Track current providers for logging
        self._current_stt_provider: STTProviderId | None = None
        self._current_llm_provider: LLMProviderId | None = None

    async def handle_client_message(self, msg_type: str, data: dict[str, Any]) -> bool:
        """Handle a client message from RTVIProcessor.

        Args:
            msg_type: The message type (e.g., "set-stt-provider")
            data: The message data payload

        Returns:
            True if the message was handled as a config message
        """
        # Only handle config messages
        if msg_type not in {
            "set-stt-provider",
            "set-llm-provider",
            "set-prompt-sections",
            "set-stt-timeout",
            "get-available-providers",
        }:
            return False

        logger.debug(f"Received config message: type={msg_type}")

        if msg_type == "set-stt-provider":
            await self._switch_stt_provider(data.get("provider"))
        elif msg_type == "set-llm-provider":
            await self._switch_llm_provider(data.get("provider"))
        elif msg_type == "set-prompt-sections":
            await self._set_prompt_sections(data.get("sections"))
        elif msg_type == "set-stt-timeout":
            await self._set_stt_timeout(data.get("timeout_seconds"))
        elif msg_type == "get-available-providers":
            await self._get_available_providers()

        return True

    async def _switch_stt_provider(self, provider_value: str | None) -> None:
        """Switch to a different STT provider.

        Args:
            provider_value: The provider ID string (e.g., "deepgram", "whisper")
        """
        if not provider_value:
            await self._send_config_error("stt-provider", "Provider value is required")
            return

        try:
            provider_id = STTProviderId(provider_value)
        except ValueError:
            await self._send_config_error("stt-provider", f"Unknown provider: {provider_value}")
            return

        if provider_id not in self._stt_services:
            await self._send_config_error(
                "stt-provider",
                f"Provider '{provider_value}' not available (no API key configured)",
            )
            return

        service = self._stt_services[provider_id]
        await self._stt_switcher.process_frame(
            ManuallySwitchServiceFrame(service=service),
            FrameDirection.DOWNSTREAM,
        )
        self._current_stt_provider = provider_id

        logger.success(f"Switched STT provider to: {provider_value}")
        await self._send_config_success("stt-provider", provider_value)

    async def _switch_llm_provider(self, provider_value: str | None) -> None:
        """Switch to a different LLM provider.

        Args:
            provider_value: The provider ID string (e.g., "openai", "anthropic")
        """
        if not provider_value:
            await self._send_config_error("llm-provider", "Provider value is required")
            return

        try:
            provider_id = LLMProviderId(provider_value)
        except ValueError:
            await self._send_config_error("llm-provider", f"Unknown provider: {provider_value}")
            return

        if provider_id not in self._llm_services:
            await self._send_config_error(
                "llm-provider",
                f"Provider '{provider_value}' not available (no API key configured)",
            )
            return

        service = self._llm_services[provider_id]
        await self._llm_switcher.process_frame(
            ManuallySwitchServiceFrame(service=service),
            FrameDirection.DOWNSTREAM,
        )
        self._current_llm_provider = provider_id

        logger.success(f"Switched LLM provider to: {provider_value}")
        await self._send_config_success("llm-provider", provider_value)

    async def _set_prompt_sections(self, sections: dict[str, Any] | None) -> None:
        """Update the LLM formatting prompt sections.

        Args:
            sections: The prompt sections configuration, or None to reset to defaults.
        """
        if not sections:
            # Reset to default
            self._llm_converter.set_prompt_sections()
            logger.info("Reset formatting prompt to default")
            await self._send_config_success("prompt-sections", "default")
            return

        try:
            self._llm_converter.set_prompt_sections(
                main_custom=sections.get("main", {}).get("content"),
                advanced_enabled=sections.get("advanced", {}).get("enabled", True),
                advanced_custom=sections.get("advanced", {}).get("content"),
                dictionary_enabled=sections.get("dictionary", {}).get("enabled", False),
                dictionary_custom=sections.get("dictionary", {}).get("content"),
            )
            await self._send_config_success("prompt-sections", "custom")
        except Exception as e:
            logger.error(f"Failed to set prompt sections: {e}")
            await self._send_config_error("prompt-sections", str(e))

    async def _set_stt_timeout(self, timeout_seconds: float | None) -> None:
        """Set the STT transcription timeout.

        Args:
            timeout_seconds: The timeout value in seconds
        """
        if timeout_seconds is None:
            await self._send_config_error("stt-timeout", "Timeout value is required")
            return

        if timeout_seconds < 0.1 or timeout_seconds > 10.0:
            await self._send_config_error(
                "stt-timeout", "Timeout must be between 0.1 and 10.0 seconds"
            )
            return

        self._transcription_buffer.set_transcription_timeout(timeout_seconds)
        logger.info(f"Set STT timeout to: {timeout_seconds}s")
        await self._send_config_success("stt-timeout", timeout_seconds)

    async def _get_available_providers(self) -> None:
        """Return available providers with model info from instantiated services.

        This returns accurate model information from the per-client service
        instances, ensuring each client sees their own configuration.
        """
        from services.provider_registry import get_llm_provider_labels, get_stt_provider_labels

        stt_labels = get_stt_provider_labels()
        llm_labels = get_llm_provider_labels()

        stt_providers = [
            {
                "value": provider_id.value,
                "label": stt_labels.get(provider_id, provider_id.value),
                "is_local": provider_id == STTProviderId.WHISPER,
                "model": getattr(service, "model_name", None),
            }
            for provider_id, service in self._stt_services.items()
        ]

        llm_providers = [
            {
                "value": provider_id.value,
                "label": llm_labels.get(provider_id, provider_id.value),
                "is_local": provider_id == LLMProviderId.OLLAMA,
                "model": getattr(service, "model_name", None),
            }
            for provider_id, service in self._llm_services.items()
        ]

        frame = RTVIServerMessageFrame(
            data={
                "type": "available-providers",
                "stt": stt_providers,
                "llm": llm_providers,
            }
        )
        await self._rtvi.push_frame(frame, FrameDirection.DOWNSTREAM)
        logger.debug(
            f"Sent available providers: {len(stt_providers)} STT, {len(llm_providers)} LLM"
        )

    async def _send_config_success(self, setting: str, value: Any) -> None:
        """Send a configuration success message to the client.

        Args:
            setting: The setting that was updated
            value: The new value
        """
        frame = RTVIServerMessageFrame(
            data={
                "type": "config-updated",
                "setting": setting,
                "value": value,
                "success": True,
            }
        )
        await self._rtvi.push_frame(frame, FrameDirection.DOWNSTREAM)

    async def _send_config_error(self, setting: str, error: str) -> None:
        """Send a configuration error message to the client.

        Args:
            setting: The setting that failed to update
            error: The error message
        """
        frame = RTVIServerMessageFrame(
            data={
                "type": "config-error",
                "setting": setting,
                "error": error,
            }
        )
        await self._rtvi.push_frame(frame, FrameDirection.DOWNSTREAM)
        logger.warning(f"Config error for {setting}: {error}")
