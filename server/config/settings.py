"""Configuration management for Tambourine server using Pydantic Settings."""

from typing import Self

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
    aws_region: str | None = Field(None, description="AWS region for Transcribe")
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

        return self
