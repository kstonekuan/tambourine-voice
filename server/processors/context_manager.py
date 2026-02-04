"""Dictation-specific context management wrapping LLMContextAggregatorPair.

This module provides a context manager that integrates pipecat's LLMContextAggregatorPair
with the dictation-specific requirements:
- Three-section prompt system (main/advanced/dictionary)
- Context reset before each recording (no conversation history)
- External turn control via UserStartedSpeakingFrame/UserStoppedSpeakingFrame
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from openai.types.chat import ChatCompletionSystemMessageParam
from pipecat.processors.aggregators.llm_context import LLMContext
from pipecat.processors.aggregators.llm_response_universal import (
    LLMAssistantAggregatorParams,
    LLMContextAggregatorPair,
    LLMUserAggregatorParams,
)
from pipecat.turns.user_turn_strategies import ExternalUserTurnStrategies

from processors.llm import combine_prompt_sections
from utils.logger import logger

if TYPE_CHECKING:
    from pipecat.processors.aggregators.llm_response_universal import (
        LLMAssistantAggregator,
        LLMUserAggregator,
    )


class DictationContextManager:
    """Manages LLM context for dictation with custom prompt support.

    Wraps LLMContextAggregatorPair and provides:
    - Three-section prompt system (main/advanced/dictionary)
    - Context reset before each recording
    - Aggregator access for pipeline placement

    The aggregator pair uses ExternalUserTurnStrategies, meaning turn boundaries
    are controlled externally via UserStartedSpeakingFrame/UserStoppedSpeakingFrame
    emitted by TranscriptionBufferProcessor.
    """

    def __init__(self, **kwargs: Any) -> None:
        """Initialize the dictation context manager."""
        # Prompt section configuration (same structure as TranscriptionToLLMConverter)
        self._main_custom: str | None = None
        self._advanced_enabled: bool = True
        self._advanced_custom: str | None = None
        self._dictionary_enabled: bool = True
        self._dictionary_custom: str | None = None

        # Create shared context (will be reset before each recording)
        self._context = LLMContext()

        # Create aggregator pair with external turn control
        # External strategies mean TranscriptionBufferProcessor controls when turns start/stop
        self._aggregator_pair = LLMContextAggregatorPair(
            self._context,
            user_params=LLMUserAggregatorParams(
                user_turn_strategies=ExternalUserTurnStrategies(),
                user_turn_stop_timeout=10.0,  # Long timeout since we control stops externally
            ),
            assistant_params=LLMAssistantAggregatorParams(),
        )

    @property
    def system_prompt(self) -> str:
        """Get the combined system prompt from all sections."""
        return combine_prompt_sections(
            main_custom=self._main_custom,
            advanced_enabled=self._advanced_enabled,
            advanced_custom=self._advanced_custom,
            dictionary_enabled=self._dictionary_enabled,
            dictionary_custom=self._dictionary_custom,
        )

    def set_prompt_sections(
        self,
        main_custom: str | None = None,
        advanced_enabled: bool = True,
        advanced_custom: str | None = None,
        dictionary_enabled: bool = False,
        dictionary_custom: str | None = None,
    ) -> None:
        """Update the prompt sections.

        The main section is always enabled. For each section, provide a custom
        prompt to override the default, or None to use the default.

        Args:
            main_custom: Custom prompt for main section, or None for default.
            advanced_enabled: Whether the advanced section is enabled.
            advanced_custom: Custom prompt for advanced section, or None for default.
            dictionary_enabled: Whether the dictionary section is enabled.
            dictionary_custom: Custom prompt for dictionary section, or None for default.
        """
        self._main_custom = main_custom
        self._advanced_enabled = advanced_enabled
        self._advanced_custom = advanced_custom
        self._dictionary_enabled = dictionary_enabled
        self._dictionary_custom = dictionary_custom
        logger.info("Formatting prompt sections updated")

    def reset_context_for_new_recording(self) -> None:
        """Reset the context for a new recording session.

        Called by TranscriptionBufferProcessor when recording starts.
        Clears all previous messages and sets the system prompt.
        This ensures each dictation is independent with no conversation history.
        """
        self._context.set_messages(
            [ChatCompletionSystemMessageParam(role="system", content=self.system_prompt)]
        )
        logger.debug("Context reset for new recording")

    async def reset_aggregator(self) -> None:
        """Reset the user aggregator's internal buffer.

        This clears any accumulated transcriptions that haven't been processed.
        Should be called when starting a new recording to prevent text leakage
        from previous recordings (especially when LLM was disabled).
        """
        await self._aggregator_pair.user().reset()
        logger.debug("User aggregator buffer reset")

    def user_aggregator(self) -> LLMUserAggregator:
        """Get the user aggregator for pipeline placement.

        The user aggregator collects transcriptions between UserStartedSpeakingFrame
        and UserStoppedSpeakingFrame, then emits LLMContextFrame to trigger LLM.
        """
        return self._aggregator_pair.user()

    def assistant_aggregator(self) -> LLMAssistantAggregator:
        """Get the assistant aggregator for pipeline placement.

        The assistant aggregator collects LLM responses and adds them to context.
        For dictation, we don't need response history, but this maintains
        compatibility with pipecat's expected pipeline structure.
        """
        return self._aggregator_pair.assistant()
