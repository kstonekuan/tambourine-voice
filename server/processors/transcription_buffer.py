"""Transcription buffer processor for dictation.

Buffers transcription text until the user explicitly stops recording,
then emits a single consolidated transcription for LLM formatting.

Uses a state machine pattern with tagged unions for explicit state management:
- IdleState: Not recording
- RecordingState: Actively buffering transcriptions
- WaitingForSTTState: Stop received, waiting for STT to catch up
"""

from __future__ import annotations

import asyncio
from dataclasses import dataclass
from datetime import UTC, datetime
from typing import Any, Final

from pipecat.frames.frames import (
    Frame,
    TranscriptionFrame,
    UserStartedSpeakingFrame,
    UserStoppedSpeakingFrame,
)
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor
from pipecat.processors.frameworks.rtvi import RTVIServerMessageFrame
from pipecat.transcriptions.language import Language

from processors.frames import FinalizeSTTFrame
from utils.logger import logger

# Default timeout for waiting for STT transcriptions (can be overridden at runtime)
DEFAULT_TRANSCRIPTION_WAIT_TIMEOUT_SECONDS: Final[float] = 0.5


# =============================================================================
# State Machine Types
# =============================================================================


@dataclass(frozen=True)
class IdleState:
    """Not recording. Waiting for start-recording message."""

    pass


@dataclass(frozen=True)
class RecordingState:
    """Actively recording and buffering transcriptions."""

    buffer: str = ""
    user_id: str = "user"
    language: Language | None = None
    speech_detected: bool = False


@dataclass(frozen=True)
class WaitingForSTTState:
    """Stop-recording received, waiting for VAD to signal speech has stopped.

    This state is entered when speech was detected when stop-recording arrives.
    We wait for UserStoppedSpeakingFrame from VAD to ensure all pending STT
    transcriptions have been delivered before emitting the final buffer.
    """

    buffer: str
    user_id: str
    language: Language | None
    direction: FrameDirection


@dataclass(frozen=True)
class DrainingState:
    """Speech stopped, draining any remaining transcriptions from STT.

    This state is entered after UserStoppedSpeakingFrame is received while
    in WaitingForSTTState. We wait briefly for late-arriving transcriptions
    before emitting the final buffer. Uses an adaptive timeout that resets
    when transcriptions arrive.
    """

    buffer: str
    user_id: str
    language: Language | None
    direction: FrameDirection


# Tagged union of all possible states
State = IdleState | RecordingState | WaitingForSTTState | DrainingState


# =============================================================================
# Processor
# =============================================================================


class TranscriptionBufferProcessor(FrameProcessor):
    """Buffers transcriptions until user stops recording.

    Uses a state machine to manage the recording lifecycle explicitly.
    State transitions are handled via pattern matching, making invalid
    states unrepresentable.
    """

    def __init__(self, **kwargs: Any) -> None:
        """Initialize the transcription buffer processor."""
        super().__init__(**kwargs)
        self._state: State = IdleState()
        self._timeout_task: asyncio.Task[None] | None = None
        self._draining_task: asyncio.Task[None] | None = None
        self._draining_event: asyncio.Event = asyncio.Event()
        # Configurable timeout for waiting for STT transcriptions (can be updated at runtime)
        self._transcription_wait_timeout = DEFAULT_TRANSCRIPTION_WAIT_TIMEOUT_SECONDS

    def set_transcription_timeout(self, seconds: float) -> None:
        """Set the transcription wait timeout.

        Args:
            seconds: Timeout in seconds to wait for STT transcription.
                     Increase for slower STT providers.
        """
        self._transcription_wait_timeout = seconds
        logger.info(f"Transcription timeout set to {seconds}s")

    def get_transcription_timeout(self) -> float:
        """Get the current transcription wait timeout."""
        return self._transcription_wait_timeout

    async def cleanup(self) -> None:
        """Clean up processor resources including internal tasks.

        Called by pipecat when the pipeline is being shut down.
        Cancels any pending timeout or draining tasks.
        """
        self._cancel_timeout()
        self._cancel_draining()
        await super().cleanup()

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frames using state machine pattern."""
        await super().process_frame(frame, direction)

        # Handle speech detection
        if isinstance(frame, UserStartedSpeakingFrame):
            self._handle_speech_started()
            await self.push_frame(frame, direction)
            return

        if isinstance(frame, UserStoppedSpeakingFrame):
            await self._handle_speech_stopped(direction)
            await self.push_frame(frame, direction)
            return

        # Handle transcription
        if isinstance(frame, TranscriptionFrame):
            if frame.text:
                await self._handle_transcription(frame, direction)
            return

        # Pass through all other frames unchanged
        await self.push_frame(frame, direction)

    # =========================================================================
    # Public API for RTVI Event Handler
    # =========================================================================

    async def start_recording(self) -> None:
        """Start recording - called from RTVI on_client_message handler."""
        await self._handle_start_recording()

    async def stop_recording(self, direction: FrameDirection = FrameDirection.DOWNSTREAM) -> None:
        """Stop recording - called from RTVI on_client_message handler."""
        await self._handle_stop_recording(direction)

    # =========================================================================
    # State Transition Handlers
    # =========================================================================

    async def _handle_start_recording(self) -> None:
        """Transition to RecordingState from any state."""
        # Cancel any pending tasks from previous states
        self._cancel_timeout()
        self._cancel_draining()
        logger.info("Start-recording received, entering RecordingState")
        self._state = RecordingState()

    async def _handle_stop_recording(self, direction: FrameDirection) -> None:
        """Handle stop-recording based on current state."""
        match self._state:
            case RecordingState(buffer=buffer, speech_detected=speech_detected) as state:
                if speech_detected:
                    # Speech detected - wait for VAD to signal speech stopped
                    # This handles the race condition where STT still has pending
                    # transcriptions when stop-recording arrives
                    logger.info(
                        f"Stop-recording received, waiting for speech to stop "
                        f"(buffer: '{buffer.strip()}')"
                    )
                    # Signal STT to finalize any pending transcription
                    # This handles the case where user stops recording mid-speech
                    # (before VAD detects silence)
                    await self.push_frame(FinalizeSTTFrame(), FrameDirection.UPSTREAM)
                    self._state = WaitingForSTTState(
                        buffer=state.buffer,
                        user_id=state.user_id,
                        language=state.language,
                        direction=direction,
                    )
                    self._timeout_task = asyncio.create_task(self._stt_timeout_handler(direction))
                else:
                    # No speech detected - send empty response
                    logger.info("Stop-recording received, no speech detected, sending empty")
                    await self._emit_empty_response(direction)
                    self._state = IdleState()

            case WaitingForSTTState():
                # Already waiting - ignore duplicate stop
                logger.warning("Stop-recording received while already waiting for STT")

            case IdleState():
                # Not recording - send empty response
                logger.warning("Stop-recording received while idle")
                await self._emit_empty_response(direction)

    def _handle_speech_started(self) -> None:
        """Mark that speech was detected in current recording."""
        match self._state:
            case RecordingState() as state:
                self._state = RecordingState(
                    buffer=state.buffer,
                    user_id=state.user_id,
                    language=state.language,
                    speech_detected=True,
                )
            case _:
                pass  # Ignore speech events in other states

    async def _handle_speech_stopped(self, direction: FrameDirection) -> None:
        """Handle speech stopped from VAD based on current state."""
        match self._state:
            case WaitingForSTTState(buffer=buffer) as state:
                # Speech stopped while waiting - enter draining state to catch
                # late transcriptions that may still be coming from STT
                self._cancel_timeout()
                logger.info(f"Speech stopped, entering draining state (buffer: '{buffer.strip()}')")
                self._state = DrainingState(
                    buffer=state.buffer,
                    user_id=state.user_id,
                    language=state.language,
                    direction=state.direction,
                )
                # Start draining task with adaptive timeout
                self._draining_event.clear()
                self._draining_task = asyncio.create_task(
                    self._draining_task_handler(state.direction)
                )
            case RecordingState():
                # Normal speech stopped during recording - just clear the flag
                # (speech can start/stop multiple times during a recording session)
                pass
            case IdleState():
                pass  # Ignore when idle
            case DrainingState():
                pass  # Already draining, ignore

    async def _handle_transcription(
        self, frame: TranscriptionFrame, direction: FrameDirection
    ) -> None:
        """Handle incoming transcription based on current state."""
        match self._state:
            case RecordingState() as state:
                # Accumulate transcription
                new_buffer = state.buffer + frame.text
                self._state = RecordingState(
                    buffer=new_buffer,
                    user_id=frame.user_id,
                    language=frame.language,
                    speech_detected=state.speech_detected,
                )
                logger.debug(f"Buffered transcription: '{frame.text}' (total: '{new_buffer}')")

            case WaitingForSTTState() as state:
                # Transcription arrived while waiting for speech to stop
                # Add to buffer and continue waiting
                new_buffer = state.buffer + frame.text
                logger.info(f"Transcription arrived while waiting: '{new_buffer.strip()}'")
                self._state = WaitingForSTTState(
                    buffer=new_buffer,
                    user_id=frame.user_id,
                    language=frame.language,
                    direction=state.direction,
                )

            case DrainingState() as state:
                # Late transcription arrived during draining - add to buffer
                # and reset the draining timeout
                new_buffer = state.buffer + frame.text
                logger.info(f"Late transcription during draining: '{frame.text}'")
                self._state = DrainingState(
                    buffer=new_buffer,
                    user_id=frame.user_id,
                    language=frame.language,
                    direction=state.direction,
                )
                # Signal the draining task to reset its timeout
                self._draining_event.set()

            case IdleState():
                # Ignore transcriptions when idle (shouldn't happen)
                logger.warning(f"Received transcription while idle: '{frame.text}'")

    # =========================================================================
    # Timeout Handler
    # =========================================================================

    async def _stt_timeout_handler(self, direction: FrameDirection) -> None:
        """Background task that emits buffer after timeout if speech stopped is not received."""
        try:
            await asyncio.sleep(self._transcription_wait_timeout)
            # Only act if still in WaitingForSTT state
            match self._state:
                case WaitingForSTTState(buffer=buffer) as state:
                    logger.warning(
                        f"Timeout waiting for speech stopped after "
                        f"{self._transcription_wait_timeout}s"
                    )
                    if buffer.strip():
                        logger.info(f"Timeout, emitting buffer: '{buffer.strip()}'")
                        await self._emit_transcription(state, state.direction)
                    else:
                        await self._emit_empty_response(direction)
                    self._state = IdleState()
                case _:
                    pass  # State changed, nothing to do
        except asyncio.CancelledError:
            pass  # Cancelled by speech stopped or new recording

    def _cancel_timeout(self) -> None:
        """Cancel any pending timeout task."""
        if self._timeout_task and not self._timeout_task.done():
            self._timeout_task.cancel()
            self._timeout_task = None

    # =========================================================================
    # Draining Handler
    # =========================================================================

    async def _draining_task_handler(self, direction: FrameDirection) -> None:
        """Wait for late transcriptions with adaptive timeout, then emit.

        Uses an event-based pattern: waits for the transcription timeout, but
        resets the timer each time a transcription arrives (signaled via
        _draining_event). Emits when the timeout expires with no new transcriptions.

        Uses the user-configurable transcription timeout to handle slow STT providers.
        """
        try:
            while True:
                await asyncio.wait_for(
                    self._draining_event.wait(),
                    timeout=self._transcription_wait_timeout,
                )
                # Transcription arrived - clear event and wait again
                self._draining_event.clear()
        except TimeoutError:
            # No transcription for draining timeout - emit now
            match self._state:
                case DrainingState(buffer=buffer) as state:
                    if buffer.strip():
                        logger.info(f"Draining complete, emitting: '{buffer.strip()}'")
                        await self._emit_transcription(state, state.direction)
                    else:
                        logger.info("Draining complete with empty buffer, sending empty")
                        await self._emit_empty_response(direction)
                    self._state = IdleState()
                case _:
                    pass  # State changed, nothing to do
        except asyncio.CancelledError:
            pass  # Cancelled by new recording

    def _cancel_draining(self) -> None:
        """Cancel any pending draining task."""
        if self._draining_task and not self._draining_task.done():
            self._draining_task.cancel()
            self._draining_task = None
        self._draining_event.clear()

    # =========================================================================
    # Output Helpers
    # =========================================================================

    async def _emit_transcription(
        self,
        state: RecordingState | WaitingForSTTState | DrainingState,
        direction: FrameDirection,
    ) -> None:
        """Emit the buffered transcription as a consolidated frame."""
        consolidated_frame = TranscriptionFrame(
            text=state.buffer.strip(),
            user_id=state.user_id,
            timestamp=datetime.now(UTC).isoformat(),
            language=state.language,
        )
        await self.push_frame(consolidated_frame, direction)

    async def _emit_empty_response(self, direction: FrameDirection) -> None:
        """Send an empty response message to the client."""
        frame = RTVIServerMessageFrame(data={"type": "recording-complete", "hasContent": False})
        await self.push_frame(frame, direction)
