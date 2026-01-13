# Original source: https://github.com/pipecat-ai/nemotron-january-2026
# File: pipecat_bots/nvidia_stt.py
# This is a custom service - may be replaced with official Pipecat support in the future
#
# NVIDIA WebSocket STT Service for Pipecat
#
# Connects to NVIDIA Parakeet ASR server via WebSocket for streaming transcription.
#

"""NVIDIA Parakeet streaming speech-to-text service implementation."""

import asyncio
import contextlib
import json
import time
from collections.abc import AsyncGenerator
from typing import Any

import websockets
from loguru import logger
from pipecat.frames.frames import (
    CancelFrame,
    EndFrame,
    ErrorFrame,
    Frame,
    InterimTranscriptionFrame,
    MetricsFrame,
    StartFrame,
    TranscriptionFrame,
    UserStartedSpeakingFrame,
    UserStoppedSpeakingFrame,
    VADUserStoppedSpeakingFrame,
)
from pipecat.metrics.metrics import TTFBMetricsData
from pipecat.processors.frame_processor import FrameDirection
from pipecat.services.stt_service import WebsocketSTTService
from pipecat.utils.time import time_now_iso8601

from processors.frames import FinalizeSTTFrame


class NVidiaWebSocketSTTService(WebsocketSTTService):
    """NVIDIA Parakeet streaming speech-to-text service.

    Provides real-time speech recognition using NVIDIA's Parakeet ASR model
    via WebSocket. Supports interim results for responsive transcription.

    The server expects:
    - Audio: 16-bit PCM, 16kHz, mono
    - Reset signal: {"type": "reset"} to finalize current utterance

    The server sends:
    - Ready: {"type": "ready"}
    - Transcript: {"type": "transcript", "text": "...", "is_final": true/false}
    """

    def __init__(
        self,
        *,
        url: str = "ws://localhost:8080",
        sample_rate: int = 16000,
        **kwargs: Any,
    ) -> None:
        """Initialize the NVIDIA STT service.

        Args:
            url: WebSocket URL of the NVIDIA ASR server.
            sample_rate: Audio sample rate (must be 16000 for Parakeet).
            **kwargs: Additional arguments passed to the parent WebsocketSTTService.
        """
        super().__init__(sample_rate=sample_rate, **kwargs)
        self._url = url
        self._websocket = None
        self._receive_task: asyncio.Task | None = None
        self._ready = False
        # Lock to ensure any in-progress audio send completes before reset
        self._audio_send_lock = asyncio.Lock()
        # Diagnostic: track audio bytes sent since last reset
        self._audio_bytes_sent = 0

        # Frame ordering fix: hold UserStoppedSpeakingFrame until final transcript arrives
        # This prevents the 500ms aggregator timeout when transcript arrives after UserStoppedSpeaking
        self._waiting_for_final: bool = False
        self._pending_user_stopped_frame: UserStoppedSpeakingFrame | None = None
        self._pending_frame_direction: FrameDirection = FrameDirection.DOWNSTREAM
        self._pending_frame_timeout_task: asyncio.Task | None = None
        self._pending_frame_timeout_s: float = 0.5  # 500ms fallback timeout

        # STT processing time metric: VADUserStoppedSpeaking -> final transcript
        self._vad_stopped_time: float | None = None

    def can_generate_metrics(self) -> bool:
        """Check if this service can generate processing metrics."""
        return True

    async def start(self, frame: StartFrame) -> None:
        """Start the NVIDIA STT service.

        Args:
            frame: The start frame containing initialization parameters.
        """
        await super().start(frame)
        await self._connect()

    async def stop(self, frame: EndFrame) -> None:
        """Stop the NVIDIA STT service.

        Args:
            frame: The end frame.
        """
        # Clean up pending frame state
        await self._cancel_pending_frame_timeout()
        if self._pending_user_stopped_frame:
            await self.push_frame(self._pending_user_stopped_frame, self._pending_frame_direction)
            self._pending_user_stopped_frame = None
        # Send HARD reset to ensure any buffered audio is transcribed
        await self._send_reset(finalize=True)
        await super().stop(frame)
        await self._disconnect()

    async def cancel(self, frame: CancelFrame) -> None:
        """Cancel the NVIDIA STT service.

        Args:
            frame: The cancel frame.
        """
        # Clean up pending frame state (discard on cancel)
        await self._cancel_pending_frame_timeout()
        self._pending_user_stopped_frame = None
        self._waiting_for_final = False
        # Send HARD reset to capture any remaining buffered audio
        # This ensures words at the end of audio aren't lost when pipeline is cancelled
        await self._send_reset(finalize=True)
        # Wait briefly for server to process the reset and send response
        # Without this, we disconnect before receiving the final transcript
        if self._websocket and self._ready:
            try:
                msg = await asyncio.wait_for(self._websocket.recv(), timeout=0.5)
                data = json.loads(msg)
                if data.get("type") == "transcript" and data.get("is_final"):
                    await self._handle_transcript(data)
            except (TimeoutError, Exception):
                pass  # Best effort - don't block cancel on network issues
        await super().cancel(frame)
        await self._disconnect()

    async def run_stt(self, audio: bytes) -> AsyncGenerator[Frame]:  # type: ignore[override]
        """Send audio data to NVIDIA ASR server for transcription.

        Args:
            audio: Raw audio bytes (16-bit PCM, 16kHz, mono).

        Yields:
            Frame: None (transcription results come via WebSocket receive task).
        """
        if self._websocket and self._ready:
            try:
                async with self._audio_send_lock:
                    self._audio_bytes_sent += len(audio)
                    await self._websocket.send(audio)
            except Exception as e:
                logger.error(f"{self} failed to send audio: {e}")
                await self._report_error(ErrorFrame(f"Failed to send audio: {e}"))
        yield None

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frames with NVIDIA-specific handling.

        Implements frame ordering fix to ensure TranscriptionFrame arrives at the
        aggregator before UserStoppedSpeakingFrame. This prevents the 500ms
        aggregation timeout that occurs when frames arrive in the wrong order.

        Args:
            frame: The frame to process.
            direction: The direction of frame processing.
        """
        # Handle UserStartedSpeakingFrame - reset pending frame state
        if isinstance(frame, UserStartedSpeakingFrame):
            await self._cancel_pending_frame_timeout()
            self._pending_user_stopped_frame = None
            self._waiting_for_final = False
            self._vad_stopped_time = None
            await super().process_frame(frame, direction)
            return

        # Handle FinalizeSTTFrame - recording is ending, send hard reset
        # This handles the case where user stops recording mid-speech (no VAD silence)
        if isinstance(frame, FinalizeSTTFrame):
            self._waiting_for_final = True
            self._vad_stopped_time = time.time()
            await self._send_reset(finalize=True)
            return

        # Handle UserStoppedSpeakingFrame - hold it and send hard reset
        if isinstance(frame, UserStoppedSpeakingFrame):
            if self._waiting_for_final:
                # Hold this frame until final transcript arrives from hard reset
                self._pending_user_stopped_frame = frame
                self._pending_frame_direction = direction
                self._start_pending_frame_timeout()
                self._vad_stopped_time = time.time()
                await self._send_reset(finalize=True)
                return  # Don't pass through yet
            # If not waiting for final, pass through normally
            await super().process_frame(frame, direction)
            return

        # All other frames pass through normally
        await super().process_frame(frame, direction)

        # Trigger SOFT reset on VAD silence detection.
        # VAD fires after ~200ms of silence, so all speech audio has already
        # been sent. Soft reset returns current text quickly without forcing
        # decoder finalization (no keep_all_outputs=True), preserving decoder state.
        if isinstance(frame, VADUserStoppedSpeakingFrame):
            self._waiting_for_final = True
            await self._send_reset(finalize=False)  # Soft reset

    async def _send_reset(self, finalize: bool = True) -> None:
        """Send reset signal to trigger transcription.

        Args:
            finalize: If True (hard reset), server adds padding and uses
                      keep_all_outputs=True to capture trailing words.
                      If False (soft reset), server returns current text
                      without forcing decoder output.

        Acquires audio_send_lock to ensure any in-progress audio send completes
        before the reset signal is sent.
        """
        if self._websocket and self._ready:
            try:
                async with self._audio_send_lock:
                    await self._websocket.send(json.dumps({"type": "reset", "finalize": finalize}))
                    if finalize:
                        self._audio_bytes_sent = 0  # Reset counter on hard reset
            except Exception as e:
                logger.error(f"{self} failed to send reset: {e}")

    def _start_pending_frame_timeout(self) -> None:
        """Start timeout task to release pending UserStoppedSpeakingFrame.

        If the final transcript doesn't arrive within the timeout, we release
        the held frame anyway to prevent the pipeline from getting stuck.
        """
        if self._pending_frame_timeout_task:
            self._pending_frame_timeout_task.cancel()
        self._pending_frame_timeout_task = asyncio.create_task(
            self._pending_frame_timeout_handler()
        )

    async def _pending_frame_timeout_handler(self) -> None:
        """Handle timeout for pending UserStoppedSpeakingFrame."""
        try:
            await asyncio.sleep(self._pending_frame_timeout_s)
            if self._pending_user_stopped_frame:
                await self.push_frame(
                    self._pending_user_stopped_frame, self._pending_frame_direction
                )
                self._pending_user_stopped_frame = None
                self._waiting_for_final = False
        except asyncio.CancelledError:
            pass

    async def _cancel_pending_frame_timeout(self) -> None:
        """Cancel the pending frame timeout task."""
        if self._pending_frame_timeout_task:
            self._pending_frame_timeout_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._pending_frame_timeout_task
            self._pending_frame_timeout_task = None

    async def _release_pending_frame(self) -> None:
        """Release the pending UserStoppedSpeakingFrame after final transcript.

        Always resets _waiting_for_final since the final transcript has arrived.
        If UserStoppedSpeakingFrame arrives later, it should pass through normally.
        """
        # Always reset waiting state - the final transcript has arrived
        self._waiting_for_final = False

        if self._pending_user_stopped_frame:
            await self._cancel_pending_frame_timeout()
            await self.push_frame(self._pending_user_stopped_frame, self._pending_frame_direction)
            self._pending_user_stopped_frame = None

    async def _connect(self) -> None:
        """Connect to the NVIDIA ASR service."""
        await self._connect_websocket()

        # Start receive task
        self._receive_task = asyncio.create_task(self._receive_task_handler(self._report_error))

        await self._call_event_handler("on_connected", self)

    async def _disconnect(self) -> None:
        """Disconnect from the NVIDIA ASR service."""
        # Cancel receive task
        if self._receive_task:
            self._receive_task.cancel()
            with contextlib.suppress(asyncio.CancelledError):
                await self._receive_task
            self._receive_task = None

        await self._disconnect_websocket()
        await self._call_event_handler("on_disconnected", self)

    async def _connect_websocket(self) -> None:
        """Establish the websocket connection.

        Uses longer timeouts to handle model loading on first connection.
        """
        try:
            self._websocket = await websockets.connect(
                self._url,
                open_timeout=60.0,  # Allow time for model loading on first connection
            )
            self._ready = False

            # Wait for ready message (30s to allow for model warmup)
            try:
                ready_msg = await asyncio.wait_for(self._websocket.recv(), timeout=30.0)
                data = json.loads(ready_msg)
                if data.get("type") == "ready":
                    self._ready = True
                    logger.info(f"{self} connected and ready")
                else:
                    logger.warning(f"{self} unexpected initial message: {data}")
                    self._ready = True  # Proceed anyway
            except TimeoutError:
                logger.warning(f"{self} timeout waiting for ready message, proceeding anyway")
                self._ready = True

        except Exception as e:
            logger.error(f"{self} connection failed: {e}")
            await self._report_error(ErrorFrame(f"Connection failed: {e}"))
            raise

    async def _disconnect_websocket(self) -> None:
        """Close the websocket connection."""
        self._ready = False
        if self._websocket:
            try:
                await self._websocket.close()
            except Exception as e:
                logger.debug(f"{self} error closing websocket: {e}")
            finally:
                self._websocket = None

    async def _receive_messages(self) -> None:
        """Receive and process websocket messages from NVIDIA ASR server."""
        if not self._websocket:
            return

        async for message in self._websocket:
            try:
                data = json.loads(message)
                msg_type = data.get("type")

                if msg_type == "transcript":
                    await self._handle_transcript(data)
                elif msg_type == "error":
                    error_msg = data.get("message", "Unknown error")
                    logger.error(f"{self} server error: {error_msg}")
                    await self._report_error(ErrorFrame(f"Server error: {error_msg}"))
                elif msg_type == "ready":
                    self._ready = True

            except json.JSONDecodeError as e:
                logger.error(f"{self} invalid JSON: {e}")
            except Exception as e:
                logger.error(f"{self} error processing message: {e}")

    async def _handle_transcript(self, data: dict[str, Any]) -> None:
        """Handle a transcript message from the server.

        For final transcripts from HARD resets (finalize=True), releases any
        pending UserStoppedSpeakingFrame AFTER pushing the transcript. This
        ensures correct frame ordering at the aggregator, preventing the 500ms
        timeout.

        For SOFT resets (finalize=False), we push the transcript but don't
        release the pending frame - we're still waiting for the hard reset
        response that will have the complete text including trailing words.

        Args:
            data: The transcript message data.
        """
        text = data.get("text", "")
        is_final = data.get("is_final", False)
        is_hard_reset = data.get("finalize", True)  # Default True for backward compat

        if not text:
            # Even with empty text, release pending frame on hard reset
            if is_final and is_hard_reset:
                await self._release_pending_frame()
            return

        await self.stop_ttfb_metrics()

        timestamp = time_now_iso8601()

        if is_final:
            if is_hard_reset:
                # Server handles deduplication - it sends only the delta (new portion)
                # so we emit directly without client-side deduplication
                if text:
                    await self.push_frame(
                        TranscriptionFrame(
                            text,
                            self._user_id,
                            timestamp,
                            language=None,
                        )
                    )
                    await self.stop_processing_metrics()

                    # Emit STT processing time metric
                    if self._vad_stopped_time is not None:
                        processing_time = time.time() - self._vad_stopped_time
                        metrics_frame = MetricsFrame(
                            data=[
                                TTFBMetricsData(
                                    processor="NemotronSTT",
                                    value=processing_time,
                                )
                            ]
                        )
                        await self.push_frame(metrics_frame)
                        self._vad_stopped_time = None

                # Release pending UserStoppedSpeakingFrame
                await self._release_pending_frame()
        else:
            await self.push_frame(
                InterimTranscriptionFrame(
                    text,
                    self._user_id,
                    timestamp,
                    language=None,
                )
            )

    async def start_metrics(self) -> None:
        """Start TTFB and processing metrics collection."""
        await self.start_ttfb_metrics()
        await self.start_processing_metrics()
