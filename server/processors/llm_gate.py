"""LLM Gate Filter - Controls frame flow to the LLM aggregator.

This filter sits between TurnController and the LLMUserAggregator to:
1. Own the LLM bypass state (single source of truth)
2. Gate frames selectively for the aggregator
3. Emit RawTranscriptionMessage when recording ends with LLM bypassed
4. Auto-skip LLM for short/clean transcriptions to reduce latency

Key insight: The aggregator only accumulates frames between UserStartedSpeakingFrame
and UserStoppedSpeakingFrame. By blocking UserStartedSpeakingFrame, we prevent
accumulation while still letting TranscriptionFrames flow through for RTVI
UserTranscript events.

Pipeline position:
    TurnController → LLMGateFilter → LLMUserAggregator
"""

from __future__ import annotations

import re
import unicodedata
from typing import Any, Final

from pipecat.frames.frames import (
    Frame,
    TranscriptionFrame,
    UserStartedSpeakingFrame,
    UserStoppedSpeakingFrame,
)
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor
from pipecat.processors.frameworks.rtvi import RTVIServerMessageFrame

from protocol.messages import EmptyTranscriptMessage, RawTranscriptionMessage
from utils.logger import logger

# Short-text auto-skip thresholds
_SHORT_TEXT_WORD_THRESHOLD: Final[int] = 8  # For Latin text
_SHORT_TEXT_CHAR_THRESHOLD: Final[int] = 20  # For CJK text

# CJK Unicode ranges for detecting Chinese/Japanese/Korean text
_CJK_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"[\u4e00-\u9fff\u3400-\u4dbf\uf900-\ufaff]"
)


def _is_short_clean_text(text: str) -> bool:
    """Check if text is short enough to skip LLM formatting.

    Uses word count for Latin text and character count for CJK text.
    """
    cjk_chars = len(_CJK_PATTERN.findall(text))
    total_chars = len(text)

    if cjk_chars > total_chars * 0.3:
        # CJK-heavy text: use character count
        return total_chars <= _SHORT_TEXT_CHAR_THRESHOLD
    else:
        # Latin text: use word count
        return len(text.split()) <= _SHORT_TEXT_WORD_THRESHOLD


class LLMGateFilter(FrameProcessor):
    """Gates frames to LLM aggregator and handles bypass output.

    When LLM formatting is disabled:
    - Blocks UserStartedSpeakingFrame (aggregator stays idle)
    - Passes TranscriptionFrame through (RTVI gets UserTranscript events)
    - Blocks UserStoppedSpeakingFrame and emits RawTranscriptionMessage instead

    When LLM formatting is enabled:
    - For short/clean text: auto-skips LLM and emits raw transcription
    - For longer text: passes all frames through to LLM
    """

    def __init__(self, **kwargs: Any) -> None:
        """Initialize the LLM gate filter."""
        super().__init__(**kwargs)
        self._llm_formatting_enabled: bool = True
        self._auto_skip_short: bool = True
        self._accumulated_text: list[str] = []
        self._started_speaking_sent: bool = False

    def set_llm_formatting_enabled(self, enabled: bool) -> None:
        """Set whether LLM formatting is enabled.

        Args:
            enabled: True to use LLM formatting, False for raw transcription
        """
        self._llm_formatting_enabled = enabled
        if enabled:
            logger.info("LLM formatting enabled (LLMGateFilter)")
        else:
            logger.info("LLM formatting disabled (LLMGateFilter)")

    def get_llm_formatting_enabled(self) -> bool:
        """Get whether LLM formatting is enabled."""
        return self._llm_formatting_enabled

    def reset_for_recording(self) -> None:
        """Reset state for a new recording.

        Called when recording starts to clear any accumulated text.
        """
        self._accumulated_text = []
        self._started_speaking_sent = False

    async def _emit_raw(self, text: str, direction: FrameDirection) -> None:
        """Emit text as RawTranscriptionMessage, bypassing LLM."""
        if text:
            await self.push_frame(
                RTVIServerMessageFrame(
                    data=RawTranscriptionMessage(text=text).model_dump()
                ),
                direction,
            )
        else:
            await self.push_frame(
                RTVIServerMessageFrame(data=EmptyTranscriptMessage().model_dump()),
                direction,
            )

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frames, gating them based on LLM formatting state."""
        await super().process_frame(frame, direction)

        if not self._llm_formatting_enabled:
            # LLM explicitly bypassed - selective gating
            match frame:
                case UserStartedSpeakingFrame():
                    self._accumulated_text = []
                    self._started_speaking_sent = False
                    logger.debug("LLM bypassed: blocking UserStartedSpeakingFrame")

                case TranscriptionFrame(text=text) if text:
                    self._accumulated_text.append(text)
                    await self.push_frame(frame, direction)

                case UserStoppedSpeakingFrame():
                    combined_text = " ".join(self._accumulated_text).strip()
                    logger.info(f"LLM bypassed: emitting raw transcription: '{combined_text}'")
                    await self._emit_raw(combined_text, direction)
                    self._accumulated_text = []

                case _:
                    await self.push_frame(frame, direction)

        elif self._auto_skip_short:
            # LLM enabled + auto-skip: defer UserStartedSpeaking, decide at turn end
            match frame:
                case UserStartedSpeakingFrame():
                    # Defer - don't send to aggregator yet
                    self._accumulated_text = []
                    self._started_speaking_sent = False
                    logger.debug("Auto-skip: deferring UserStartedSpeakingFrame")

                case TranscriptionFrame(text=text) if text:
                    self._accumulated_text.append(text)
                    await self.push_frame(frame, direction)

                case UserStoppedSpeakingFrame():
                    combined_text = " ".join(self._accumulated_text).strip()

                    if _is_short_clean_text(combined_text):
                        # Short text - skip LLM, emit raw directly
                        logger.info(
                            f"Auto-skip: short text, emitting raw: '{combined_text}'"
                        )
                        await self._emit_raw(combined_text, direction)
                    else:
                        # Long text - replay start + stop to aggregator for LLM
                        logger.info(
                            f"Auto-skip: long text ({len(combined_text)} chars), "
                            f"sending to LLM"
                        )
                        await self.push_frame(
                            UserStartedSpeakingFrame(), direction
                        )
                        await self.push_frame(frame, direction)

                    self._accumulated_text = []

                case _:
                    await self.push_frame(frame, direction)
        else:
            # LLM enabled, no auto-skip - pass everything through
            await self.push_frame(frame, direction)
