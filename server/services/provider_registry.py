"""Provider registry for STT and LLM services.

This module defines the available providers using direct class imports for
compile-time type safety. If pipecat changes class names, we get import errors
at startup instead of runtime failures.
"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from enum import StrEnum
from typing import TYPE_CHECKING, Any, Final

# Direct imports from pipecat - type checked at import time
from pipecat.services.anthropic.llm import AnthropicLLMService
from pipecat.services.assemblyai.stt import AssemblyAISTTService
from pipecat.services.aws.stt import AWSTranscribeSTTService
from pipecat.services.azure.stt import AzureSTTService
from pipecat.services.cartesia.stt import CartesiaSTTService
from pipecat.services.cerebras.llm import CerebrasLLMService
from pipecat.services.deepgram.stt import DeepgramSTTService
from pipecat.services.google.llm import GoogleLLMService
from pipecat.services.google.stt import GoogleSTTService
from pipecat.services.groq.llm import GroqLLMService
from pipecat.services.groq.stt import GroqSTTService
from pipecat.services.llm_service import LLMService
from pipecat.services.ollama.llm import OLLamaLLMService
from pipecat.services.openai.llm import OpenAILLMService
from pipecat.services.openai.stt import OpenAISTTService
from pipecat.services.openrouter.llm import OpenRouterLLMService
from pipecat.services.speechmatics.stt import SpeechmaticsSTTService
from pipecat.services.stt_service import STTService
from pipecat.services.whisper.stt import WhisperSTTService

# Custom service for Nemotron ASR
from services.nvidia_stt import NVidiaWebSocketSTTService

if TYPE_CHECKING:
    from config.settings import Settings


# =============================================================================
# Provider ID Enums - Type-safe provider identifiers
# =============================================================================


class STTProviderId(StrEnum):
    """Speech-to-Text provider identifiers."""

    SPEECHMATICS = "speechmatics"
    ASSEMBLYAI = "assemblyai"
    AWS = "aws"
    AZURE = "azure"
    CARTESIA = "cartesia"
    DEEPGRAM = "deepgram"
    GOOGLE = "google"
    GROQ = "groq"
    NEMOTRON = "nemotron"
    OPENAI = "openai"
    WHISPER = "whisper"


class LLMProviderId(StrEnum):
    """Large Language Model provider identifiers."""

    ANTHROPIC = "anthropic"
    CEREBRAS = "cerebras"
    GEMINI = "gemini"
    GROQ = "groq"
    OLLAMA = "ollama"
    OPENAI = "openai"
    OPENROUTER = "openrouter"


# =============================================================================
# Credential Mappers - Map Settings fields to constructor kwargs
# =============================================================================


class CredentialMapper(ABC):
    """Abstract base for mapping Settings fields to service constructor kwargs."""

    @abstractmethod
    def get_required_fields(self) -> tuple[str, ...]:
        """Return Settings field names required for this provider."""
        ...

    @abstractmethod
    def map_credentials(self, settings: "Settings") -> dict[str, Any]:
        """Map Settings values to constructor kwargs."""
        ...

    def is_available(self, settings: "Settings") -> bool:
        """Check if all required credentials are configured."""
        return all(getattr(settings, field_name, None) for field_name in self.get_required_fields())


class ApiKeyMapper(CredentialMapper):
    """Maps a single api_key field to the 'api_key' constructor parameter."""

    def __init__(self, settings_field: str, param_name: str = "api_key") -> None:
        self.settings_field = settings_field
        self.param_name = param_name

    def get_required_fields(self) -> tuple[str, ...]:
        return (self.settings_field,)

    def map_credentials(self, settings: "Settings") -> dict[str, Any]:
        value = getattr(settings, self.settings_field, None)
        if value:
            return {self.param_name: value}
        return {}


class MultiFieldMapper(CredentialMapper):
    """Maps multiple Settings fields to constructor kwargs."""

    def __init__(
        self,
        field_mapping: dict[str, str],  # settings_field -> param_name
        required_fields: tuple[str, ...] | None = None,
    ) -> None:
        self.field_mapping = field_mapping
        self._required_fields = (
            required_fields if required_fields is not None else tuple(field_mapping.keys())
        )

    def get_required_fields(self) -> tuple[str, ...]:
        return self._required_fields

    def map_credentials(self, settings: "Settings") -> dict[str, Any]:
        result: dict[str, Any] = {}
        for settings_field, param_name in self.field_mapping.items():
            value = getattr(settings, settings_field, None)
            if value:
                result[param_name] = value
        return result


class NoAuthMapper(CredentialMapper):
    """For providers that don't require authentication (e.g., local Whisper, Ollama).

    These providers still need explicit opt-in via availability_fields to prevent
    them from appearing when not actually configured.
    """

    def __init__(
        self,
        availability_fields: tuple[str, ...] = (),
        field_mapping: dict[str, str] | None = None,
    ) -> None:
        """Initialize with availability and field mappings.

        Args:
            availability_fields: Settings fields that must be truthy for the provider
                to be considered available (e.g., ("whisper_enabled",) or ("ollama_base_url",))
            field_mapping: Mapping of settings_field -> param_name for constructor
                parameters (e.g., {"ollama_base_url": "base_url", "ollama_model": "model"})
        """
        self.availability_fields = availability_fields
        self.field_mapping = field_mapping or {}

    def get_required_fields(self) -> tuple[str, ...]:
        return ()

    def is_available(self, settings: "Settings") -> bool:
        """Check if all availability fields are set (truthy)."""
        if not self.availability_fields:
            return False  # No auth providers must explicitly opt-in
        return all(getattr(settings, field, None) for field in self.availability_fields)

    def map_credentials(self, settings: "Settings") -> dict[str, Any]:
        result: dict[str, Any] = {}
        for settings_field, param_name in self.field_mapping.items():
            value = getattr(settings, settings_field, None)
            if value:
                result[param_name] = value
        return result


# =============================================================================
# Provider Configuration Dataclasses
# =============================================================================


@dataclass(frozen=True)
class STTProviderConfig:
    """Configuration for an STT provider with direct class reference.

    Attributes:
        provider_id: Enum identifier for this provider
        display_name: Human-readable name for UI (e.g., "Deepgram")
        service_class: The actual pipecat service class (type-checked at import time)
        credential_mapper: Maps Settings fields to constructor kwargs
        default_kwargs: Additional kwargs to pass to constructor
    """

    provider_id: STTProviderId
    display_name: str
    service_class: type[STTService]
    credential_mapper: CredentialMapper
    default_kwargs: dict[str, Any] = field(default_factory=dict)


@dataclass(frozen=True)
class LLMProviderConfig:
    """Configuration for an LLM provider with direct class reference.

    Attributes:
        provider_id: Enum identifier for this provider
        display_name: Human-readable name for UI (e.g., "OpenAI")
        service_class: The actual pipecat service class (type-checked at import time)
        credential_mapper: Maps Settings fields to constructor kwargs
        default_kwargs: Additional kwargs to pass to constructor
    """

    provider_id: LLMProviderId
    display_name: str
    service_class: type[LLMService]
    credential_mapper: CredentialMapper
    default_kwargs: dict[str, Any] = field(default_factory=dict)


# =============================================================================
# STT Provider Registry
# =============================================================================

STT_PROVIDERS: Final[dict[STTProviderId, STTProviderConfig]] = {
    STTProviderId.SPEECHMATICS: STTProviderConfig(
        provider_id=STTProviderId.SPEECHMATICS,
        display_name="Speechmatics",
        service_class=SpeechmaticsSTTService,
        credential_mapper=ApiKeyMapper("speechmatics_api_key"),
        default_kwargs={
            "params": SpeechmaticsSTTService.InputParams(
                end_of_utterance_silence_trigger=0.5,
            )
        },
    ),
    STTProviderId.ASSEMBLYAI: STTProviderConfig(
        provider_id=STTProviderId.ASSEMBLYAI,
        display_name="AssemblyAI",
        service_class=AssemblyAISTTService,
        credential_mapper=ApiKeyMapper("assemblyai_api_key"),
    ),
    STTProviderId.AWS: STTProviderConfig(
        provider_id=STTProviderId.AWS,
        display_name="AWS Transcribe",
        service_class=AWSTranscribeSTTService,
        credential_mapper=MultiFieldMapper(
            {
                "aws_access_key_id": "aws_access_key_id",
                "aws_secret_access_key": "aws_secret_access_key",
                "aws_region": "region",
            },
            required_fields=("aws_access_key_id", "aws_secret_access_key"),
        ),
    ),
    STTProviderId.AZURE: STTProviderConfig(
        provider_id=STTProviderId.AZURE,
        display_name="Azure Speech",
        service_class=AzureSTTService,
        credential_mapper=MultiFieldMapper(
            {
                "azure_speech_key": "api_key",
                "azure_speech_region": "region",
            }
        ),
    ),
    STTProviderId.CARTESIA: STTProviderConfig(
        provider_id=STTProviderId.CARTESIA,
        display_name="Cartesia",
        service_class=CartesiaSTTService,
        credential_mapper=ApiKeyMapper("cartesia_api_key"),
    ),
    STTProviderId.DEEPGRAM: STTProviderConfig(
        provider_id=STTProviderId.DEEPGRAM,
        display_name="Deepgram",
        service_class=DeepgramSTTService,
        credential_mapper=ApiKeyMapper("deepgram_api_key"),
    ),
    STTProviderId.GOOGLE: STTProviderConfig(
        provider_id=STTProviderId.GOOGLE,
        display_name="Google Speech",
        service_class=GoogleSTTService,
        credential_mapper=MultiFieldMapper(
            {"google_application_credentials": "credentials_path"},
            required_fields=("google_application_credentials",),
        ),
    ),
    STTProviderId.GROQ: STTProviderConfig(
        provider_id=STTProviderId.GROQ,
        display_name="Groq",
        service_class=GroqSTTService,
        credential_mapper=ApiKeyMapper("groq_api_key"),
    ),
    STTProviderId.NEMOTRON: STTProviderConfig(
        provider_id=STTProviderId.NEMOTRON,
        display_name="Nemotron ASR",
        service_class=NVidiaWebSocketSTTService,
        credential_mapper=NoAuthMapper(
            availability_fields=("nemotron_asr_url",),
            field_mapping={"nemotron_asr_url": "url"},
        ),
    ),
    STTProviderId.OPENAI: STTProviderConfig(
        provider_id=STTProviderId.OPENAI,
        display_name="OpenAI",
        service_class=OpenAISTTService,
        credential_mapper=ApiKeyMapper("openai_api_key"),
    ),
    STTProviderId.WHISPER: STTProviderConfig(
        provider_id=STTProviderId.WHISPER,
        display_name="Whisper",
        service_class=WhisperSTTService,
        credential_mapper=NoAuthMapper(availability_fields=("whisper_enabled",)),
    ),
}


# =============================================================================
# LLM Provider Registry
# =============================================================================

LLM_PROVIDERS: Final[dict[LLMProviderId, LLMProviderConfig]] = {
    LLMProviderId.ANTHROPIC: LLMProviderConfig(
        provider_id=LLMProviderId.ANTHROPIC,
        display_name="Anthropic Claude",
        service_class=AnthropicLLMService,
        credential_mapper=ApiKeyMapper("anthropic_api_key"),
    ),
    LLMProviderId.CEREBRAS: LLMProviderConfig(
        provider_id=LLMProviderId.CEREBRAS,
        display_name="Cerebras",
        service_class=CerebrasLLMService,
        credential_mapper=ApiKeyMapper("cerebras_api_key"),
        default_kwargs={"retry_on_timeout": True, "retry_timeout_secs": 10.0},
    ),
    LLMProviderId.GEMINI: LLMProviderConfig(
        provider_id=LLMProviderId.GEMINI,
        display_name="Google Gemini",
        service_class=GoogleLLMService,
        credential_mapper=ApiKeyMapper("google_api_key"),
    ),
    LLMProviderId.GROQ: LLMProviderConfig(
        provider_id=LLMProviderId.GROQ,
        display_name="Groq",
        service_class=GroqLLMService,
        credential_mapper=ApiKeyMapper("groq_api_key"),
    ),
    LLMProviderId.OLLAMA: LLMProviderConfig(
        provider_id=LLMProviderId.OLLAMA,
        display_name="Ollama",
        service_class=OLLamaLLMService,
        credential_mapper=NoAuthMapper(
            availability_fields=("ollama_base_url", "ollama_model"),
            field_mapping={
                "ollama_base_url": "base_url",
                "ollama_model": "model",
            },
        ),
    ),
    LLMProviderId.OPENAI: LLMProviderConfig(
        provider_id=LLMProviderId.OPENAI,
        display_name="OpenAI",
        service_class=OpenAILLMService,
        credential_mapper=MultiFieldMapper(
            {
                "openai_api_key": "api_key",
                "openai_base_url": "base_url",
            },
            required_fields=("openai_api_key",),
        ),
    ),
    LLMProviderId.OPENROUTER: LLMProviderConfig(
        provider_id=LLMProviderId.OPENROUTER,
        display_name="OpenRouter",
        service_class=OpenRouterLLMService,
        credential_mapper=ApiKeyMapper("openrouter_api_key"),
    ),
}


# =============================================================================
# Helper Functions
# =============================================================================


def get_stt_provider_config(provider_id: STTProviderId) -> STTProviderConfig | None:
    """Get STT provider config by ID.

    Args:
        provider_id: The provider ID enum

    Returns:
        The provider config, or None if not found
    """
    return STT_PROVIDERS.get(provider_id)


def get_llm_provider_config(provider_id: LLMProviderId) -> LLMProviderConfig | None:
    """Get LLM provider config by ID.

    Args:
        provider_id: The provider ID enum

    Returns:
        The provider config, or None if not found
    """
    return LLM_PROVIDERS.get(provider_id)


def get_stt_provider_labels() -> dict[STTProviderId, str]:
    """Get mapping of provider_id to display_name for STT providers."""
    return {pid: config.display_name for pid, config in STT_PROVIDERS.items()}


def get_llm_provider_labels() -> dict[LLMProviderId, str]:
    """Get mapping of provider_id to display_name for LLM providers."""
    return {pid: config.display_name for pid, config in LLM_PROVIDERS.items()}
