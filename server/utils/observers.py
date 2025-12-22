"""Custom logging observer for pipeline events.

Filters frames by source to avoid duplicate logs as frames propagate through the pipeline.
"""

from pipecat.frames.frames import (
    InputAudioRawFrame,
    LLMFullResponseEndFrame,
    LLMFullResponseStartFrame,
    LLMTextFrame,
    MetricsFrame,
    StartFrame,
    TextFrame,
    TranscriptionFrame,
    UserSpeakingFrame,
    UserStartedSpeakingFrame,
    UserStoppedSpeakingFrame,
)
from pipecat.observers.base_observer import BaseObserver, FramePushed
from pipecat.processors.frameworks.rtvi import RTVIServerMessageFrame
from pipecat.services.llm_service import LLMService
from pipecat.services.stt_service import STTService
from pipecat.transports.base_input import BaseInputTransport
from pipecat.transports.base_output import BaseOutputTransport

from utils.logger import logger


class PipelineLogObserver(BaseObserver):
    """Observer that logs key pipeline events at INFO level.

    Uses source filtering to log each event only once:
    - StartFrame: logged when reaching output transport (end of pipeline)
    - Audio/Speech frames: logged when from input transport (origin)
    - Transcription: logged when from STT service (origin)
    - LLM response: logged when from LLM service (origin)
    - RTVI messages: logged when from output transport (being sent)

    Logs at DEBUG level:
    - Other frames (excluding noisy UserSpeakingFrame and MetricsFrame)
    """

    def __init__(self) -> None:
        """Initialize the observer."""
        super().__init__()
        self._llm_accumulator: str = ""
        self._is_accumulating: bool = False
        self._audio_frame_count: int = 0
        # Track speaking state to deduplicate speech events from multiple sources
        self._is_speaking: bool = False

    async def on_push_frame(self, data: FramePushed) -> None:
        """Handle frame push events and log key pipeline activities.

        Args:
            data: The frame push event data containing source, frame, and other info.
        """
        src = data.source
        frame = data.frame

        # Log pipeline start when it reaches the output transport (end of pipeline)
        if isinstance(frame, StartFrame) and isinstance(src, BaseOutputTransport):
            logger.success("Pipeline started")

        # Log audio frames from input transport (first few and periodic)
        elif isinstance(frame, InputAudioRawFrame) and isinstance(src, BaseInputTransport):
            self._audio_frame_count += 1
            if self._audio_frame_count % 500 == 0:
                logger.info(
                    f"Audio frame #{self._audio_frame_count}: "
                    f"{len(frame.audio)} bytes, {frame.sample_rate}Hz, {frame.num_channels}ch"
                )

        # Log transcription from STT service
        elif isinstance(frame, TranscriptionFrame) and isinstance(src, STTService):
            logger.info(f"TRANSCRIPTION: '{frame.text}'")

        # Log speech start/stop from input transport (where VAD runs)
        # Use state tracking to deduplicate - same event may come from multiple sources
        elif isinstance(frame, UserStartedSpeakingFrame) and isinstance(src, BaseInputTransport):
            if not self._is_speaking:
                self._is_speaking = True
                logger.info("Speech started")
        elif isinstance(frame, UserStoppedSpeakingFrame) and isinstance(src, BaseInputTransport):
            if self._is_speaking:
                self._is_speaking = False
                logger.info("Speech stopped")

        # Accumulate and log LLM response from LLM service
        # Use LLMTextFrame (not TextFrame) - this is what LLM services output
        elif isinstance(frame, LLMFullResponseStartFrame) and isinstance(src, LLMService):
            self._llm_accumulator = ""
            self._is_accumulating = True
        elif (
            isinstance(frame, LLMTextFrame)
            and self._is_accumulating
            and isinstance(src, LLMService)
        ):
            self._llm_accumulator += frame.text
        elif isinstance(frame, LLMFullResponseEndFrame) and isinstance(src, LLMService):
            self._is_accumulating = False
            if self._llm_accumulator.strip():
                logger.info(f"Cleaned text: '{self._llm_accumulator.strip()}'")
            self._llm_accumulator = ""

        # Log RTVI server messages when sent from output transport
        elif isinstance(frame, RTVIServerMessageFrame) and isinstance(src, BaseOutputTransport):
            logger.info(f"Sending to client: {frame.data}")

        # Log other frames at debug level (skip noisy ones)
        elif not isinstance(frame, (UserSpeakingFrame, MetricsFrame, TextFrame, LLMTextFrame)):
            logger.debug(f"Frame: {type(frame).__name__}")
