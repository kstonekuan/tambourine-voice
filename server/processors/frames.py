"""Custom Pipecat frames for Tambourine."""

from dataclasses import dataclass

from pipecat.frames.frames import SystemFrame


@dataclass
class FinalizeSTTFrame(SystemFrame):
    """Frame to signal that the recording session is ending.

    When this frame is received, STT services should finalize any pending
    transcription. This handles the case where the user stops recording
    mid-speech (before VAD detects silence).
    """

    pass
