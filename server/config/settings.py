"""Configuration management for Tambourine server using Pydantic Settings."""

from typing import Self

from loguru import logger
from pydantic import Field, model_validator
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """Application configuration settings loaded from environment variables."""

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        extra="ignore",
        case_sensitive=False,
    )

    # STT API Keys (at least one required)
    assemblyai_api_key: str | None = Field(None, description="AssemblyAI API key for STT")
    cartesia_api_key: str | None = Field(None, description="Cartesia API key for STT")
    deepgram_api_key: str | None = Field(None, description="Deepgram API key for STT")
    speechmatics_api_key: str | None = Field(None, description="Speechmatics API key for STT")
    aws_access_key_id: str | None = Field(None, description="AWS access key ID for Transcribe")
    aws_secret_access_key: str | None = Field(
        None, description="AWS secret access key for Transcribe"
    )
    aws_session_token: str | None = Field(
        None, description="AWS session token for Bedrock or Transcribe (optional)"
    )
    aws_region: str | None = Field(None, description="AWS region for Bedrock or Transcribe")
    azure_speech_key: str | None = Field(None, description="Azure Speech API key")
    azure_speech_region: str | None = Field(None, description="Azure Speech region")
    whisper_enabled: bool = Field(
        False, description="Enable local Whisper STT (requires model setup)"
    )
    nemotron_asr_url: str | None = Field(
        None, description="Nemotron ASR WebSocket URL (ws:// or wss://)"
    )

    # LLM API Keys (at least one required)
    openai_api_key: str | None = Field(None, description="OpenAI API key for LLM")
    openai_base_url: str | None = Field(
        None, description="OpenAI base URL (optional, for OpenAI-compatible endpoints)"
    )
    google_api_key: str | None = Field(None, description="Google API key for Gemini LLM")
    anthropic_api_key: str | None = Field(None, description="Anthropic API key for LLM")
    cerebras_api_key: str | None = Field(None, description="Cerebras API key for LLM")
    groq_api_key: str | None = Field(None, description="Groq API key for LLM")
    google_application_credentials: str | None = Field(
        None, description="Path to Google service account JSON for Vertex AI and Google Speech"
    )
    ollama_base_url: str | None = Field(
        None, description="Ollama base URL (default: http://localhost:11434)"
    )
    ollama_model: str | None = Field(
        None, description="Ollama model name (e.g., llama3.2, mistral, qwen2.5)"
    )
    openrouter_api_key: str | None = Field(None, description="OpenRouter API key for LLM")
    aws_bedrock_model_id: str | None = Field(
        None, description="AWS Bedrock model ID (required to enable Bedrock)"
    )

    # Auto provider configuration (resolved when client sends "auto")
    auto_stt_provider: str | None = Field(
        default=None,
        description="Default STT provider for 'auto' mode (e.g., 'deepgram')",
    )
    auto_llm_provider: str | None = Field(
        default=None,
        description="Default LLM provider for 'auto' mode (e.g., 'cerebras')",
    )

    # Logging
    log_level: str = Field("INFO", description="Logging level")

    # Server Configuration (optional, has defaults)
    host: str = Field("127.0.0.1", description="Host to bind the server to")
    port: int = Field(8765, description="Port to listen on")

    @model_validator(mode="after")
    def validate_at_least_one_provider(self) -> Self:
        """Validate that at least one STT and one LLM provider is configured.

        Uses the provider registry to dynamically check availability and
        generate error messages with current provider names.
        """
        # Lazy import to avoid circular dependency (registry imports pipecat services)
        from services.provider_registry import LLM_PROVIDERS, STT_PROVIDERS

        # Check STT providers using registry's credential mappers
        available_stt = [
            config.display_name
            for config in STT_PROVIDERS.values()
            if config.credential_mapper.is_available(self)
        ]
        if not available_stt:
            all_stt_names = [config.display_name for config in STT_PROVIDERS.values()]
            raise ValueError(
                f"No STT provider configured. "
                f"Configure credentials for at least one of: {', '.join(all_stt_names)}"
            )

        # Check LLM providers using registry's credential mappers
        available_llm = [
            config.display_name
            for config in LLM_PROVIDERS.values()
            if config.credential_mapper.is_available(self)
        ]
        if not available_llm:
            all_llm_names = [config.display_name for config in LLM_PROVIDERS.values()]
            raise ValueError(
                f"No LLM provider configured. "
                f"Configure credentials for at least one of: {', '.join(all_llm_names)}"
            )

        # Validate auto providers have credentials configured
        from services.provider_registry import LLMProviderId, STTProviderId

        if self.auto_stt_provider is not None:
            # Validate it's a known provider ID
            try:
                stt_provider_id = STTProviderId(self.auto_stt_provider)
            except ValueError:
                valid_ids = [p.value for p in STTProviderId]
                raise ValueError(
                    f"Invalid AUTO_STT_PROVIDER: '{self.auto_stt_provider}'. "
                    f"Must be one of: {', '.join(valid_ids)}"
                ) from None
            # Validate credentials are available
            stt_config = STT_PROVIDERS.get(stt_provider_id)
            if stt_config and not stt_config.credential_mapper.is_available(self):
                raise ValueError(
                    f"AUTO_STT_PROVIDER is set to '{self.auto_stt_provider}' but "
                    f"credentials for {stt_config.display_name} are not configured"
                )
            logger.info(
                f"Auto STT provider: {stt_config.display_name if stt_config else self.auto_stt_provider}"
            )

        if self.auto_llm_provider is not None:
            # Validate it's a known provider ID
            try:
                llm_provider_id = LLMProviderId(self.auto_llm_provider)
            except ValueError:
                valid_ids = [p.value for p in LLMProviderId]
                raise ValueError(
                    f"Invalid AUTO_LLM_PROVIDER: '{self.auto_llm_provider}'. "
                    f"Must be one of: {', '.join(valid_ids)}"
                ) from None
            # Validate credentials are available
            llm_config = LLM_PROVIDERS.get(llm_provider_id)
            if llm_config and not llm_config.credential_mapper.is_available(self):
                raise ValueError(
                    f"AUTO_LLM_PROVIDER is set to '{self.auto_llm_provider}' but "
                    f"credentials for {llm_config.display_name} are not configured"
                )
            logger.info(
                f"Auto LLM provider: {llm_config.display_name if llm_config else self.auto_llm_provider}"
            )

        return self
