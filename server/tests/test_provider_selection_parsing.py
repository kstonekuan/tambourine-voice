from protocol.providers import (
    KnownLLMProvider,
    KnownSTTProvider,
    LLMProviderId,
    OtherLLMProvider,
    OtherSTTProvider,
    STTProviderId,
    parse_llm_provider_selection,
    parse_stt_provider_selection,
)


def test_parse_known_llm_provider_selection_returns_known_provider() -> None:
    parsed_llm_provider_selection = parse_llm_provider_selection("cerebras")
    assert isinstance(parsed_llm_provider_selection, KnownLLMProvider)
    assert parsed_llm_provider_selection.provider_id == LLMProviderId.CEREBRAS


def test_parse_unknown_llm_provider_selection_returns_other_provider() -> None:
    parsed_llm_provider_selection = parse_llm_provider_selection("future-llm-provider")
    assert isinstance(parsed_llm_provider_selection, OtherLLMProvider)
    assert parsed_llm_provider_selection.provider_id == "future-llm-provider"


def test_parse_known_stt_provider_selection_returns_known_provider() -> None:
    parsed_stt_provider_selection = parse_stt_provider_selection("deepgram")
    assert isinstance(parsed_stt_provider_selection, KnownSTTProvider)
    assert parsed_stt_provider_selection.provider_id == STTProviderId.DEEPGRAM


def test_parse_known_mlx_stt_provider_selection_returns_known_provider() -> None:
    parsed_stt_provider_selection = parse_stt_provider_selection("whisper_mlx")
    assert isinstance(parsed_stt_provider_selection, KnownSTTProvider)
    assert parsed_stt_provider_selection.provider_id == STTProviderId.WHISPER_MLX


def test_parse_unknown_stt_provider_selection_returns_other_provider() -> None:
    parsed_stt_provider_selection = parse_stt_provider_selection("future-stt-provider")
    assert isinstance(parsed_stt_provider_selection, OtherSTTProvider)
    assert parsed_stt_provider_selection.provider_id == "future-stt-provider"
