"""Frame processor that forwards VAD detection events into pipeline frames.

This processor hosts a `VADController` and translates speech state events into
VAD system frames for downstream consumers that are currently outside the
`LLMUserAggregator` turn controller path.
"""

from __future__ import annotations

from typing import Any

from pipecat.audio.vad.vad_analyzer import VADAnalyzer
from pipecat.audio.vad.vad_controller import VADController
from pipecat.frames.frames import (
    Frame,
    UserSpeakingFrame,
    VADUserStartedSpeakingFrame,
    VADUserStoppedSpeakingFrame,
)
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor


class VADFrameForwardingProcessor(FrameProcessor):
    """Forwards VAD events from a `VADAnalyzer` into pipeline frames.

    The processor creates a dedicated `VADController` and connects event handlers
    so downstream processors receive VAD start/stop/speaking activity frames.
    """

    def __init__(self, vad_analyzer: VADAnalyzer, **kwargs: Any) -> None:
        """Initialize the processor.

        Args:
            vad_analyzer: VAD analyzer instance to use for speech detection.
            **kwargs: Additional arguments passed to base frame processor.
        """
        super().__init__(**kwargs)

        self._vad_analyzer = vad_analyzer
        self._vad_controller = VADController(vad_analyzer)
        self._vad_controller.add_event_handler("on_speech_started", self._on_vad_speech_started)
        self._vad_controller.add_event_handler("on_speech_stopped", self._on_vad_speech_stopped)
        self._vad_controller.add_event_handler("on_speech_activity", self._on_vad_speech_activity)
        self._vad_controller.add_event_handler("on_broadcast_frame", self._on_vad_broadcast_frame)

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frame and feed it through VAD controller."""
        await super().process_frame(frame, direction)
        # Keep the original frame flowing through the normal pipeline exactly once.
        # Forwarding the frame first preserves required StartFrame ordering, while
        # we rely on VAD broadcast callbacks for VAD-only control frames.
        await self.push_frame(frame, direction)
        await self._vad_controller.process_frame(frame)

    async def _on_vad_speech_started(self, controller: object) -> None:
        """Handle speech start and emit a `VADUserStartedSpeakingFrame`."""
        _ = controller
        await self.broadcast_frame(
            VADUserStartedSpeakingFrame,
            start_secs=self._vad_analyzer.params.start_secs,
        )

    async def _on_vad_speech_stopped(self, controller: object) -> None:
        """Handle speech stop and emit a `VADUserStoppedSpeakingFrame`."""
        _ = controller
        await self.broadcast_frame(
            VADUserStoppedSpeakingFrame,
            stop_secs=self._vad_analyzer.params.stop_secs,
        )

    async def _on_vad_speech_activity(self, controller: object) -> None:
        """Handle speech activity and emit a generic speaking frame."""
        _ = controller
        await self.broadcast_frame(UserSpeakingFrame)

    async def _on_vad_broadcast_frame(
        self,
        controller: object,
        frame_cls: type[Frame],
        **kwargs: object,
    ) -> None:
        """Forward VAD broadcast-frame requests into the current pipeline."""
        _ = controller
        await self.broadcast_frame(frame_cls, **kwargs)
