"""LLM Gate Filter - Controls frame flow to the LLM aggregator.

This filter sits between TurnController and the LLMUserAggregator to:
1. Own the LLM bypass state (single source of truth)
2. Gate frames selectively for the aggregator
3. Emit RawTranscriptionMessage when recording ends with LLM bypassed

Key insight: The aggregator only accumulates frames between UserStartedSpeakingFrame
and UserStoppedSpeakingFrame. By blocking UserStartedSpeakingFrame, we prevent
accumulation while still letting TranscriptionFrames flow through for RTVI
UserTranscript events.

Pipeline position:
    TurnController → LLMGateFilter → LLMUserAggregator
"""

from __future__ import annotations

from typing import Any

from pipecat.frames.frames import (
    Frame,
    TranscriptionFrame,
    UserStartedSpeakingFrame,
    UserStoppedSpeakingFrame,
)
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor
from pipecat.processors.frameworks.rtvi import RTVIServerMessageFrame

from protocol.messages import RawTranscriptionMessage, RecordingCompleteMessage
from utils.logger import logger


class LLMGateFilter(FrameProcessor):
    """Gates frames to LLM aggregator and handles bypass output.

    When LLM formatting is disabled:
    - Blocks UserStartedSpeakingFrame (aggregator stays idle)
    - Passes TranscriptionFrame through (RTVI gets UserTranscript events)
    - Blocks UserStoppedSpeakingFrame and emits RawTranscriptionMessage instead

    When LLM formatting is enabled:
    - Passes all frames through unchanged
    """

    def __init__(self, **kwargs: Any) -> None:
        """Initialize the LLM gate filter."""
        super().__init__(**kwargs)
        self._llm_formatting_enabled: bool = True
        self._accumulated_text: list[str] = []

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

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frames, gating them based on LLM formatting state."""
        await super().process_frame(frame, direction)

        if not self._llm_formatting_enabled:
            # LLM bypassed - selective gating
            match frame:
                case UserStartedSpeakingFrame():
                    # Block - aggregator should not start accumulating
                    self._accumulated_text = []
                    logger.debug("LLM bypassed: blocking UserStartedSpeakingFrame")

                case TranscriptionFrame(text=text) if text:
                    # Accumulate text for raw output
                    self._accumulated_text.append(text)
                    # Pass through for RTVI UserTranscript events
                    await self.push_frame(frame, direction)

                case UserStoppedSpeakingFrame():
                    # Emit raw transcription instead of passing to aggregator
                    combined_text = " ".join(self._accumulated_text).strip()
                    logger.info(f"LLM bypassed: emitting raw transcription: '{combined_text}'")

                    if combined_text:
                        await self.push_frame(
                            RTVIServerMessageFrame(
                                data=RawTranscriptionMessage(text=combined_text).model_dump()
                            ),
                            direction,
                        )
                    else:
                        await self.push_frame(
                            RTVIServerMessageFrame(
                                data=RecordingCompleteMessage(hasContent=False).model_dump()
                            ),
                            direction,
                        )

                    self._accumulated_text = []

                case _:
                    # Pass through all other frames
                    await self.push_frame(frame, direction)
        else:
            # LLM enabled - pass everything through
            await self.push_frame(frame, direction)
