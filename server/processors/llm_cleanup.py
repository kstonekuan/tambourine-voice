"""LLM-based text cleanup processor for dictation using idiomatic Pipecat patterns."""

from typing import Any

from pipecat.frames.frames import (
    Frame,
    LLMFullResponseEndFrame,
    LLMFullResponseStartFrame,
    OutputTransportMessageFrame,
    TextFrame,
    TranscriptionFrame,
)
from pipecat.processors.aggregators.openai_llm_context import (
    OpenAILLMContext,
    OpenAILLMContextFrame,
)
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor

from utils.logger import logger

# Main prompt section - Core rules, punctuation, new lines
MAIN_PROMPT_DEFAULT = """You are a dictation cleanup assistant. Your task is to clean up transcribed speech.

## Core Rules
- Remove filler words (um, uh, like, you know, basically, etc.)
- Fix grammar and punctuation
- Capitalize sentences properly
- Keep the original meaning and tone intact
- Do NOT add any new information or change the intent
- Output ONLY the cleaned text, nothing else - no explanations, no quotes, no prefixes

## Punctuation Commands
Convert spoken punctuation to symbols:
- "comma" → ,
- "period" or "full stop" → .
- "question mark" → ?
- "exclamation point" or "exclamation mark" → !
- "dash" → -
- "em dash" → —
- "quotation mark" or "quote" or "end quote" → "
- "colon" → :
- "semicolon" → ;
- "open parenthesis" or "open paren" → (
- "close parenthesis" or "close paren" → )

Example:
- "I can't wait exclamation point Let's meet at seven period" → "I can't wait! Let's meet at seven."

## New Lines and Paragraphs
- "new line" → Insert a line break
- "new paragraph" → Insert a paragraph break (blank line)

Example:
- "First point new line second point new paragraph next section" →
  "First point
  Second point

  Next section"

## Cleanup Example
Input: "um so basically I was like thinking we should uh you know update the readme file"
Output: "I was thinking we should update the readme file." """

# Advanced prompt section - Backtrack corrections and list formatting
ADVANCED_PROMPT_DEFAULT = """## Backtrack Corrections
When the speaker corrects themselves mid-sentence, use only the corrected version:
- "actually" signals a correction: "at 2 actually 3" → "at 3"
- "scratch that" removes the previous phrase: "cookies scratch that brownies" → "brownies"
- "wait" or "I mean" signal corrections: "on Monday wait Tuesday" → "on Tuesday"
- Natural restatements: "as a gift... as a present" → "as a present"

Examples:
- "Let's do coffee at 2 actually 3" → "Let's do coffee at 3."
- "I'll bring cookies scratch that brownies" → "I'll bring brownies."
- "Send it to John I mean Jane" → "Send it to Jane."

## List Formatting
When sequence words are detected, format as a numbered or bulleted list:
- Triggers: "one", "two", "three" or "first", "second", "third"
- Capitalize each list item

Example:
- "My goals are one finish the report two send the presentation three review feedback" →
  "My goals are:
  1. Finish the report
  2. Send the presentation
  3. Review feedback" """

# Dictionary prompt section - Personal word mappings
DICTIONARY_PROMPT_DEFAULT = """## Personal Dictionary
Apply these corrections for technical terms, proper nouns, and custom words.

Entries can be in various formats - interpret flexibly:
- Explicit mappings: "ant row pick -> Anthropic" or "ant row pic = Anthropic"
- Single terms to recognize: Just "LLM" (correct phonetic mismatches)
- Natural descriptions: "The name 'Claude' should always be capitalized"

When you hear terms that sound like entries below, use the correct spelling/form.

### Entries:
Tambourine
LLM
ant row pick -> Anthropic
Claude
Pipecat
Tauri"""


def combine_prompt_sections(
    main_enabled: bool,
    main_content: str | None,
    advanced_enabled: bool,
    advanced_content: str | None,
    dictionary_enabled: bool,
    dictionary_content: str | None,
) -> str:
    """Combine enabled prompt sections into a single prompt."""
    parts: list[str] = []

    if main_enabled:
        content = main_content if main_content else MAIN_PROMPT_DEFAULT
        parts.append(content)

    if advanced_enabled:
        content = advanced_content if advanced_content else ADVANCED_PROMPT_DEFAULT
        parts.append(content)

    if dictionary_enabled:
        content = dictionary_content if dictionary_content else DICTIONARY_PROMPT_DEFAULT
        parts.append(content)

    return "\n\n".join(parts)


class TranscriptionToLLMConverter(FrameProcessor):
    """Converts TranscriptionFrame to OpenAILLMContextFrame for LLM cleanup.

    This processor receives accumulated transcription text and converts it
    to an LLM context with the cleanup system prompt, triggering the LLM
    service to generate cleaned text.
    """

    def __init__(self, **kwargs: Any) -> None:
        """Initialize the converter."""
        super().__init__(**kwargs)
        self._custom_prompt: str | None = None

    @property
    def system_prompt(self) -> str:
        """Get the active system prompt (custom or default)."""
        if self._custom_prompt:
            return self._custom_prompt
        # Default: main and advanced enabled, dictionary disabled
        return combine_prompt_sections(
            main_enabled=True,
            main_content=None,
            advanced_enabled=True,
            advanced_content=None,
            dictionary_enabled=False,
            dictionary_content=None,
        )

    def set_custom_prompt(self, prompt: str | None) -> None:
        """Update the custom prompt at runtime.

        Args:
            prompt: New custom prompt, or None to use default.
        """
        self._custom_prompt = prompt
        logger.info(f"Cleanup prompt updated: {'custom' if prompt else 'default'}")

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Convert transcription frames to LLM context frames.

        Args:
            frame: The frame to process
            direction: The direction of frame flow
        """
        await super().process_frame(frame, direction)

        if isinstance(frame, TranscriptionFrame):
            text = frame.text
            if text and text.strip():
                logger.debug(f"Converting transcription to LLM context: {text[:50]}...")

                # Create OpenAI-compatible context with cleanup prompt
                context = OpenAILLMContext(
                    messages=[
                        {"role": "system", "content": self.system_prompt},
                        {"role": "user", "content": text},
                    ]
                )

                # Push context frame to trigger LLM processing
                await self.push_frame(OpenAILLMContextFrame(context=context), direction)
            return

        # Pass through all other frames unchanged
        await self.push_frame(frame, direction)


class LLMResponseToRTVIConverter(FrameProcessor):
    """Aggregates LLM response and converts to RTVI message for client.

    This processor collects streamed TextFrames between LLMFullResponseStartFrame
    and LLMFullResponseEndFrame, then sends the complete cleaned text as an
    RTVI server message to the client.
    """

    def __init__(self, **kwargs: Any) -> None:
        """Initialize the response converter."""
        super().__init__(**kwargs)
        self._accumulator: str = ""
        self._is_accumulating: bool = False

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Accumulate LLM response and convert to RTVI message.

        Args:
            frame: The frame to process
            direction: The direction of frame flow
        """
        await super().process_frame(frame, direction)

        if isinstance(frame, LLMFullResponseStartFrame):
            # Start accumulating LLM response
            self._accumulator = ""
            self._is_accumulating = True
            return

        if isinstance(frame, TextFrame) and self._is_accumulating:
            # Accumulate text chunks from LLM
            self._accumulator += frame.text
            return

        if isinstance(frame, LLMFullResponseEndFrame):
            # LLM response complete - send cleaned text to client
            self._is_accumulating = False
            cleaned_text = self._accumulator.strip()

            if cleaned_text:
                logger.info(f"Cleaned text: '{cleaned_text}'")

                # Create RTVI message for client
                rtvi_message = {
                    "label": "rtvi-ai",
                    "type": "server-message",
                    "data": {"type": "transcript", "text": cleaned_text},
                }
                await self.push_frame(OutputTransportMessageFrame(message=rtvi_message), direction)

            self._accumulator = ""
            return

        # Pass through all other frames unchanged
        await self.push_frame(frame, direction)
