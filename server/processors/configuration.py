"""Configuration handler for runtime configuration via RTVI client messages.

This module provides configuration handling for the pipeline, called from
RTVIProcessor's on_client_message event handler.
"""

from __future__ import annotations

from enum import StrEnum
from typing import TYPE_CHECKING, Annotated, Any, Literal

from loguru import logger
from pipecat.frames.frames import ManuallySwitchServiceFrame
from pipecat.processors.frame_processor import FrameDirection
from pipecat.processors.frameworks.rtvi import RTVIProcessor, RTVIServerMessageFrame
from pydantic import BaseModel, Field

from services.provider_registry import LLMProviderId, STTProviderId

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


if TYPE_CHECKING:
    from pipecat.pipeline.llm_switcher import LLMSwitcher
    from pipecat.pipeline.service_switcher import ServiceSwitcher
    from pipecat.services.ai_services import STTService
    from pipecat.services.llm_service import LLMService

    from config.settings import Settings
    from processors.context_manager import DictationContextManager
    from processors.turn_controller import TurnController


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
        context_manager: DictationContextManager,
        turn_controller: TurnController,
        stt_services: dict[STTProviderId, STTService],
        llm_services: dict[LLMProviderId, LLMService],
        settings: Settings,
    ) -> None:
        """Initialize the configuration handler.

        Args:
            rtvi_processor: The RTVIProcessor to send responses through
            stt_switcher: ServiceSwitcher for STT services
            llm_switcher: LLMSwitcher for LLM services
            context_manager: DictationContextManager for prompt configuration
            turn_controller: TurnController for timeout configuration
            stt_services: Dictionary mapping STT provider IDs to services
            llm_services: Dictionary mapping LLM provider IDs to services
            settings: Application settings for auto provider configuration
        """
        self._rtvi = rtvi_processor
        self._stt_switcher = stt_switcher
        self._llm_switcher = llm_switcher
        self._context_manager = context_manager
        self._turn_controller = turn_controller
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
            "set-prompt-sections": lambda: self._set_prompt_sections(data.get("sections")),
            "set-stt-timeout": lambda: self._set_stt_timeout(data.get("timeout_seconds")),
            "get-available-providers": self._send_available_providers,
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

    async def _set_prompt_sections(self, sections: dict[str, Any] | None) -> None:
        """Update the LLM formatting prompt sections.

        Args:
            sections: The prompt sections configuration, or None to reset to defaults.
        """
        if not sections:
            self._context_manager.set_prompt_sections()
            logger.info("Reset formatting prompt to default")
            await self._send_config_success("prompt-sections", "default")
            return

        try:
            parsed = CleanupPromptSections.model_validate(sections)

            # Extract content from discriminated union (None for auto mode)
            def get_content(section: PromptSectionAuto | PromptSectionManual) -> str | None:
                match section:
                    case PromptSectionAuto():
                        return None
                    case PromptSectionManual(content=content):
                        return content

            self._context_manager.set_prompt_sections(
                main_custom=get_content(parsed.main),
                advanced_enabled=parsed.advanced.enabled,
                advanced_custom=get_content(parsed.advanced),
                dictionary_enabled=parsed.dictionary.enabled,
                dictionary_custom=get_content(parsed.dictionary),
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

        self._turn_controller.set_transcription_timeout(timeout_seconds)
        logger.info(f"Set STT timeout to: {timeout_seconds}s")
        await self._send_config_success("stt-timeout", timeout_seconds)

    async def _send_available_providers(self) -> None:
        """Send available providers with model info from instantiated services."""
        from services.provider_registry import get_llm_provider_labels, get_stt_provider_labels

        stt_providers = self._build_provider_list(
            services=self._stt_services,
            labels=get_stt_provider_labels(),
            local_provider_ids={STTProviderId.WHISPER},
        )
        llm_providers = self._build_provider_list(
            services=self._llm_services,
            labels=get_llm_provider_labels(),
            local_provider_ids={LLMProviderId.OLLAMA},
        )

        frame = RTVIServerMessageFrame(
            data={
                "type": "available-providers",
                "stt": stt_providers,
                "llm": llm_providers,
            }
        )
        await self._rtvi.push_frame(frame)
        logger.debug(
            f"Sent available providers: {len(stt_providers)} STT, {len(llm_providers)} LLM"
        )

    def _build_provider_list(
        self,
        services: dict[Any, Any],
        labels: dict[Any, str],
        local_provider_ids: set[Any],
    ) -> list[dict[str, Any]]:
        """Build a provider info list for the client.

        Args:
            services: Dictionary mapping provider IDs to service instances
            labels: Dictionary mapping provider IDs to display labels
            local_provider_ids: Set of provider IDs that are local (not cloud)

        Returns:
            List of provider info dictionaries
        """
        return [
            {
                "value": provider_id.value,
                "label": labels.get(provider_id, provider_id.value),
                "is_local": provider_id in local_provider_ids,
                "model": getattr(service, "model_name", None),
            }
            for provider_id, service in services.items()
        ]

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
