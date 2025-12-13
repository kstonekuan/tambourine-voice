"""Configuration processor for handling runtime configuration via WebRTC data channel.

This processor handles configuration messages from the client, allowing runtime
configuration of the pipeline without global state or REST API endpoints.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Literal

from loguru import logger
from pipecat.frames.frames import (
    Frame,
    InputTransportMessageFrame,
    ManuallySwitchServiceFrame,
    OutputTransportMessageFrame,
    StartFrame,
)
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor
from pydantic import BaseModel, ValidationError

from processors.llm_cleanup import combine_prompt_sections
from services.provider_registry import LLMProviderId, STTProviderId

# =============================================================================
# Transport Message Models (Pydantic) - matches RTVI protocol
# =============================================================================


class ClientMessageData(BaseModel):
    """Data payload for client-message type."""

    t: str  # The actual message type (e.g., "set-stt-provider")
    d: dict[str, Any] | None = None  # The actual payload data


class ClientMessage(BaseModel):
    """Client message wrapper from RTVI protocol."""

    type: Literal["client-message"]
    data: ClientMessageData


if TYPE_CHECKING:
    from pipecat.pipeline.llm_switcher import LLMSwitcher
    from pipecat.pipeline.service_switcher import ServiceSwitcher
    from pipecat.services.ai_services import STTService
    from pipecat.services.llm_service import LLMService

    from processors.llm_cleanup import TranscriptionToLLMConverter
    from processors.transcription_buffer import TranscriptionBufferProcessor


class ConfigurationProcessor(FrameProcessor):
    """Handles configuration messages from WebRTC data channel.

    Processes messages like:
    - set-stt-provider: Switch STT service
    - set-llm-provider: Switch LLM service
    - set-prompt-sections: Update LLM prompt
    - set-stt-timeout: Update transcription timeout

    All configuration is scoped to this pipeline instance, eliminating
    global state and enabling multi-client support.
    """

    def __init__(
        self,
        stt_switcher: ServiceSwitcher,
        llm_switcher: LLMSwitcher,
        llm_converter: TranscriptionToLLMConverter,
        transcription_buffer: TranscriptionBufferProcessor,
        stt_services: dict[STTProviderId, STTService],
        llm_services: dict[LLMProviderId, LLMService],
        **kwargs: Any,
    ) -> None:
        """Initialize the configuration processor.

        Args:
            stt_switcher: ServiceSwitcher for STT services
            llm_switcher: LLMSwitcher for LLM services
            llm_converter: TranscriptionToLLMConverter for prompt configuration
            transcription_buffer: TranscriptionBufferProcessor for timeout configuration
            stt_services: Dictionary mapping STT provider IDs to services
            llm_services: Dictionary mapping LLM provider IDs to services
        """
        super().__init__(**kwargs)
        self._stt_switcher = stt_switcher
        self._llm_switcher = llm_switcher
        self._llm_converter = llm_converter
        self._transcription_buffer = transcription_buffer
        self._stt_services = stt_services
        self._llm_services = llm_services
        self._pipeline_started = False

        # Track current providers for logging
        self._current_stt_provider: STTProviderId | None = None
        self._current_llm_provider: LLMProviderId | None = None

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frames, handling configuration messages.

        Args:
            frame: The frame to process
            direction: The direction of frame flow
        """
        await super().process_frame(frame, direction)

        if isinstance(frame, StartFrame):
            self._pipeline_started = True
            logger.info("Pipeline started - ready to accept configuration")
        elif isinstance(frame, InputTransportMessageFrame):
            logger.debug(f"InputTransportMessageFrame received: {frame.message}")
            # Handle configuration messages from client
            handled = await self._handle_config_message(frame.message)
            if handled:
                # Don't propagate config messages to rest of pipeline
                return

        await self.push_frame(frame, direction)

    def _extract_message_type_and_data(
        self, message: dict[str, Any] | Any
    ) -> tuple[str | None, dict[str, Any]]:
        """Extract message type and data from transport message payload.

        Handles the RTVI protocol's client-message envelope format where
        messages are wrapped as:
        {
            "type": "client-message",
            "data": {
                "t": "set-stt-provider",
                "provider": "deepgram",
                ...
            }
        }

        Args:
            message: The raw message from the transport

        Returns:
            Tuple of (message_type, data_dict). Returns (None, {}) if invalid.
        """
        if not isinstance(message, dict):
            return None, {}

        # Try to parse as ClientMessage (RTVI protocol envelope)
        try:
            client_msg = ClientMessage.model_validate(message)
            # Extract the actual type from data.t and payload from data.d
            msg_type = client_msg.data.t
            payload = client_msg.data.d or {}
            return msg_type, payload
        except ValidationError:
            pass

        # Fallback: direct message format (for backwards compatibility)
        return message.get("type"), message.get("data", {})

    async def _handle_config_message(self, message: dict[str, Any]) -> bool:
        """Handle a potential configuration message.

        Args:
            message: The message dict from the client

        Returns:
            True if the message was handled as a config message
        """
        msg_type, data = self._extract_message_type_and_data(message)

        # Only handle config messages
        if msg_type not in {
            "set-stt-provider",
            "set-llm-provider",
            "set-prompt-sections",
            "set-stt-timeout",
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

        if not self._pipeline_started:
            await self._send_config_error("stt-provider", "Pipeline not ready - please try again")
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

        if not self._pipeline_started:
            await self._send_config_error("llm-provider", "Pipeline not ready - please try again")
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
        """Update the LLM cleanup prompt sections.

        Args:
            sections: The prompt sections configuration
        """
        if not sections:
            # Reset to default
            self._llm_converter.set_custom_prompt(None)
            logger.info("Reset cleanup prompt to default")
            await self._send_config_success("prompt-sections", "default")
            return

        try:
            combined = combine_prompt_sections(
                main_enabled=sections.get("main", {}).get("enabled", True),
                main_content=sections.get("main", {}).get("content"),
                advanced_enabled=sections.get("advanced", {}).get("enabled", True),
                advanced_content=sections.get("advanced", {}).get("content"),
                dictionary_enabled=sections.get("dictionary", {}).get("enabled", False),
                dictionary_content=sections.get("dictionary", {}).get("content"),
            )
            self._llm_converter.set_custom_prompt(combined if combined else None)
            logger.info("Updated cleanup prompt sections")
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

    async def _send_config_success(self, setting: str, value: Any) -> None:
        """Send a configuration success message to the client.

        Args:
            setting: The setting that was updated
            value: The new value
        """
        message = {
            "label": "rtvi-ai",
            "type": "server-message",
            "data": {
                "type": "config-updated",
                "setting": setting,
                "value": value,
                "success": True,
            },
        }
        await self.push_frame(
            OutputTransportMessageFrame(message=message),
            FrameDirection.DOWNSTREAM,
        )

    async def _send_config_error(self, setting: str, error: str) -> None:
        """Send a configuration error message to the client.

        Args:
            setting: The setting that failed to update
            error: The error message
        """
        message = {
            "label": "rtvi-ai",
            "type": "server-message",
            "data": {
                "type": "config-error",
                "setting": setting,
                "error": error,
            },
        }
        await self.push_frame(
            OutputTransportMessageFrame(message=message),
            FrameDirection.DOWNSTREAM,
        )
        logger.warning(f"Config error for {setting}: {error}")
