"""Speechmatics STT service with automatic reconnection.

Wraps pipecat's SpeechmaticsSTTService to handle stale WebSocket connections
that occur when the service sits idle behind a ServiceSwitcher. The underlying
VoiceAgentClient's session times out when no audio is sent, but the client
object remains non-None — causing TransportError on finalize().

This subclass detects a dead transport and reconnects before processing.
"""

from collections.abc import AsyncGenerator

from loguru import logger
from pipecat.frames.frames import Frame, VADUserStoppedSpeakingFrame
from pipecat.processors.frame_processor import FrameDirection
from pipecat.services.speechmatics.stt import SpeechmaticsSTTService


class ReconnectingSpeechmaticsSTTService(SpeechmaticsSTTService):
    """SpeechmaticsSTTService that reconnects when the transport dies.

    The Speechmatics VoiceAgentClient opens a WebSocket at pipeline start
    (StartFrame). If this service is not the active provider in a
    ServiceSwitcher, no audio is sent, and the server eventually closes the
    idle recognition session. The client's _closed_evt is set, but
    self._client remains non-None — so finalize() creates a fire-and-forget
    task that explodes with TransportError("Client is closed").

    This subclass checks the transport health before forwarding frames that
    would trigger finalize(), and reconnects if needed.
    """

    def _is_client_alive(self) -> bool:
        """Check whether the VoiceAgentClient transport is still usable."""
        if self._client is None:
            return False
        # _closed_evt is set by the speechmatics SDK when the WebSocket
        # receive loop exits (server close, network error, etc.)
        return not self._client._closed_evt.is_set()

    async def _reconnect(self) -> None:
        """Tear down the dead client and establish a fresh connection."""
        logger.warning(f"{self} transport is dead, reconnecting to Speechmatics")
        await self._disconnect()
        await self._connect()
        if self._client is not None and not self._client._closed_evt.is_set():
            logger.info(f"{self} reconnected successfully")
        else:
            logger.error(f"{self} reconnection failed")

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frame with transport health check before finalize.

        VADUserStoppedSpeakingFrame triggers finalize() on the client. If the
        transport is dead, reconnect first so finalize has a live session.
        """
        if isinstance(frame, VADUserStoppedSpeakingFrame) and (
            self._client is not None and not self._is_client_alive()
        ):
            await self._reconnect()

        await super().process_frame(frame, direction)

    async def run_stt(self, audio: bytes) -> AsyncGenerator[Frame]:
        """Send audio with transport health check.

        If audio frames arrive on a dead transport (e.g. immediately after
        switching to this provider), reconnect before sending.
        """
        if self._client is not None and not self._is_client_alive():
            await self._reconnect()

        async for frame in super().run_stt(audio):
            yield frame
