#!/usr/bin/env python3
"""Tambourine Server - SmallWebRTC-based Pipecat Server.

A FastAPI server that receives audio from a Tauri client via WebRTC,
processes it through STT and LLM cleanup, and returns cleaned text.

Usage:
    python main.py
    python main.py --port 8765
"""

import argparse
import asyncio
from contextlib import asynccontextmanager
from typing import Any, cast

import uvicorn
from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware
from loguru import logger
from pipecat.audio.vad.silero import SileroVADAnalyzer
from pipecat.frames.frames import (
    Frame,
    InputAudioRawFrame,
    MetricsFrame,
    OutputTransportMessageFrame,
    StartFrame,
    TranscriptionFrame,
    UserSpeakingFrame,
    UserStartedSpeakingFrame,
    UserStoppedSpeakingFrame,
)
from pipecat.pipeline.llm_switcher import LLMSwitcher
from pipecat.pipeline.pipeline import Pipeline
from pipecat.pipeline.runner import PipelineRunner
from pipecat.pipeline.service_switcher import ServiceSwitcher, ServiceSwitcherStrategyManual
from pipecat.pipeline.task import PipelineParams, PipelineTask
from pipecat.processors.frame_processor import FrameDirection, FrameProcessor
from pipecat.transports.base_transport import TransportParams
from pipecat.transports.smallwebrtc.connection import IceServer, SmallWebRTCConnection
from pipecat.transports.smallwebrtc.transport import SmallWebRTCTransport
from pydantic import BaseModel

from api.config_server import (
    config_router,
    reset_pipeline_started,
    set_llm_converter,
    set_pipeline_started,
    set_service_switchers,
    set_transcription_buffer,
)
from config.settings import Settings
from processors.llm_cleanup import LLMResponseToRTVIConverter, TranscriptionToLLMConverter
from processors.transcription_buffer import TranscriptionBufferProcessor
from services.providers import (
    LLMProviderId,
    STTProviderId,
    create_all_available_llm_services,
    create_all_available_stt_services,
)
from utils.logger import configure_logging

# Store peer connections by pc_id
peer_connections_map: dict[str, SmallWebRTCConnection] = {}

# Track running pipeline tasks for clean shutdown
pipeline_tasks: set[asyncio.Task[None]] = set()

# ICE servers for WebRTC NAT traversal
ice_servers = [
    IceServer(urls="stun:stun.l.google.com:19302"),
]

# Shared state for the pipeline components
_settings: Settings | None = None
_stt_services: dict[STTProviderId, Any] | None = None
_llm_services: dict[LLMProviderId, Any] | None = None


class DebugFrameProcessor(FrameProcessor):
    """Debug processor that logs important frames for troubleshooting.

    Filters out noisy frames (UserSpeakingFrame, MetricsFrame) and only logs
    significant events like speech start/stop and transcriptions.
    """

    def __init__(self, name: str = "debug", **kwargs: Any) -> None:
        super().__init__(**kwargs)
        self._name = name
        self._audio_frame_count = 0

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        await super().process_frame(frame, direction)

        if isinstance(frame, InputAudioRawFrame):
            self._audio_frame_count += 1
            # Only log first few and periodic audio frames
            if self._audio_frame_count <= 3 or self._audio_frame_count % 500 == 0:
                logger.info(
                    f"[{self._name}] Audio frame #{self._audio_frame_count}: "
                    f"{len(frame.audio)} bytes, {frame.sample_rate}Hz, {frame.num_channels}ch"
                )
        elif isinstance(frame, TranscriptionFrame):
            logger.info(f"[{self._name}] TRANSCRIPTION: '{frame.text}'")
        elif isinstance(frame, UserStartedSpeakingFrame):
            logger.info(f"[{self._name}] Speech started")
        elif isinstance(frame, UserStoppedSpeakingFrame):
            logger.info(f"[{self._name}] Speech stopped")
        # Skip noisy frames: UserSpeakingFrame (fires every ~15ms), MetricsFrame
        elif not isinstance(frame, (UserSpeakingFrame, MetricsFrame)):
            logger.debug(f"[{self._name}] Frame: {type(frame).__name__}")

        await self.push_frame(frame, direction)


class CleanedTextData(BaseModel):
    """Data payload containing cleaned text from LLM."""

    text: str = ""


class CleanedTextMessage(BaseModel):
    """Message containing cleaned text response."""

    data: CleanedTextData


class TextResponseProcessor(FrameProcessor):
    """Processor that logs message frames being sent back to the client.

    This processor sits at the end of the pipeline before transport.output()
    to log the final cleaned text being sent to the Tauri client.
    """

    async def process_frame(self, frame: Frame, direction: FrameDirection) -> None:
        """Process frames and log OutputTransportMessageFrames.

        Args:
            frame: The frame to process
            direction: The direction of frame flow
        """
        await super().process_frame(frame, direction)

        if isinstance(frame, StartFrame):
            # Signal that pipeline is fully started (StartFrame has passed through all processors)
            logger.success("Pipeline fully started (StartFrame passed through all processors)")
            set_pipeline_started()
        elif isinstance(frame, OutputTransportMessageFrame):
            try:
                msg = CleanedTextMessage.model_validate(frame.message)
                text = msg.data.text
            except Exception:
                text = ""
            logger.info(f"Sending to client: '{text}'")

        await self.push_frame(frame, direction)


async def run_pipeline(webrtc_connection: SmallWebRTCConnection) -> None:
    """Run the Pipecat pipeline for a single WebRTC connection.

    Args:
        webrtc_connection: The SmallWebRTCConnection instance for this client
    """
    logger.info("Starting pipeline for new WebRTC connection")

    if not _settings or not _stt_services or not _llm_services:
        logger.error("Server not properly initialized")
        return

    # Create transport using the WebRTC connection
    # audio_in_stream_on_start=False prevents timeout warnings when mic is disabled
    transport = SmallWebRTCTransport(
        webrtc_connection=webrtc_connection,
        params=TransportParams(
            audio_in_enabled=True,
            audio_out_enabled=False,  # No audio output for dictation
            audio_in_stream_on_start=False,  # Don't expect audio until client enables mic
            vad_analyzer=SileroVADAnalyzer(),
        ),
    )

    # Create service switchers for this connection
    from pipecat.pipeline.base_pipeline import FrameProcessor as PipecatFrameProcessor

    stt_service_list = cast(list[PipecatFrameProcessor], list(_stt_services.values()))
    llm_service_list = list(_llm_services.values())

    stt_switcher = ServiceSwitcher(
        services=stt_service_list,
        strategy_type=ServiceSwitcherStrategyManual,
    )

    llm_switcher = LLMSwitcher(
        llms=llm_service_list,
        strategy_type=ServiceSwitcherStrategyManual,
    )

    # Initialize processors
    debug_input = DebugFrameProcessor(name="input")
    debug_after_stt = DebugFrameProcessor(name="after-stt")
    transcription_to_llm = TranscriptionToLLMConverter()
    transcription_buffer = TranscriptionBufferProcessor()

    # Share processors with FastAPI config server for runtime configuration
    set_llm_converter(transcription_to_llm)
    set_transcription_buffer(transcription_buffer)
    set_service_switchers(
        stt_switcher=stt_switcher,
        llm_switcher=llm_switcher,
        stt_services=_stt_services,
        llm_services=_llm_services,
        settings=_settings,
    )
    llm_response_converter = LLMResponseToRTVIConverter()
    text_response = TextResponseProcessor()

    # Build pipeline
    pipeline = Pipeline(
        [
            transport.input(),
            debug_input,
            stt_switcher,
            debug_after_stt,
            transcription_buffer,
            transcription_to_llm,
            llm_switcher,
            llm_response_converter,
            text_response,
            transport.output(),
        ]
    )

    # Create pipeline task
    task = PipelineTask(
        pipeline,
        params=PipelineParams(
            allow_interruptions=False,
            enable_metrics=True,
            enable_usage_metrics=True,
        ),
        idle_timeout_secs=None,
    )

    # Set up event handlers
    @transport.event_handler("on_client_connected")
    async def on_client_connected(_transport: Any, client: Any) -> None:
        logger.success(f"Client connected via WebRTC: {client}")

    @transport.event_handler("on_client_disconnected")
    async def on_client_disconnected(_transport: Any, client: Any) -> None:
        logger.info(f"Client disconnected: {client}")
        reset_pipeline_started()
        await task.cancel()

    # Run the pipeline
    runner = PipelineRunner(handle_sigint=False)
    await runner.run(task)


def initialize_services(settings: Settings) -> bool:
    """Initialize STT and LLM services.

    Args:
        settings: Application settings

    Returns:
        True if services were initialized successfully
    """
    global _settings, _stt_services, _llm_services

    _settings = settings
    _stt_services = create_all_available_stt_services(settings)
    _llm_services = create_all_available_llm_services(settings)

    if not _stt_services:
        logger.error("No STT providers available. Configure at least one STT API key.")
        return False

    if not _llm_services:
        logger.error("No LLM providers available. Configure at least one LLM API key.")
        return False

    logger.info(f"Available STT providers: {[p.value for p in _stt_services]}")
    logger.info(f"Available LLM providers: {[p.value for p in _llm_services]}")

    return True


@asynccontextmanager
async def lifespan(_fastapi_app: FastAPI):  # noqa: ANN201
    """FastAPI lifespan context manager for cleanup."""
    yield
    logger.info("Shutting down server...")

    # Cancel all running pipeline tasks first
    for task in pipeline_tasks:
        task.cancel()
    if pipeline_tasks:
        # Wait for tasks to finish with a timeout
        try:
            await asyncio.wait_for(
                asyncio.gather(*pipeline_tasks, return_exceptions=True),
                timeout=2.0,
            )
            logger.success("All pipeline tasks cancelled")
        except TimeoutError:
            logger.warning("Timeout waiting for pipeline tasks, forcing shutdown")
        pipeline_tasks.clear()

    # Disconnect all peer connections with timeout
    coros = [pc.disconnect() for pc in peer_connections_map.values()]
    if coros:
        try:
            await asyncio.wait_for(
                asyncio.gather(*coros, return_exceptions=True),
                timeout=2.0,
            )
            logger.success("All peer connections cleaned up")
        except TimeoutError:
            logger.warning("Timeout waiting for peer connections, forcing shutdown")
    peer_connections_map.clear()


# Create FastAPI app
app = FastAPI(title="Tambourine Server", lifespan=lifespan)

# CORS for Tauri frontend
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# Include config routes
app.include_router(config_router)


# =============================================================================
# WebRTC Models
# =============================================================================


class WebRTCOfferRequest(BaseModel):
    """WebRTC offer request from client."""

    sdp: str
    type: str
    pc_id: str | None = None
    restart_pc: bool = False


class WebRTCOfferResponse(BaseModel):
    """WebRTC answer response to client."""

    sdp: str
    type: str
    pc_id: str


@app.post("/api/offer", response_model=WebRTCOfferResponse)
async def webrtc_offer(request: WebRTCOfferRequest) -> WebRTCOfferResponse:
    """Handle WebRTC offer from client.

    This endpoint handles the WebRTC signaling handshake:
    1. Receives SDP offer from client
    2. Creates or reuses a SmallWebRTCConnection
    3. Returns SDP answer to client
    4. Starts the Pipecat pipeline in the background
    """
    if request.pc_id and request.pc_id in peer_connections_map:
        # Reuse existing connection (renegotiation)
        pipecat_connection = peer_connections_map[request.pc_id]
        logger.info(f"Reusing existing connection for pc_id: {request.pc_id}")
        await pipecat_connection.renegotiate(
            sdp=request.sdp,
            type=request.type,
            restart_pc=request.restart_pc,
        )
    else:
        # Create new connection
        pipecat_connection = SmallWebRTCConnection(ice_servers)
        await pipecat_connection.initialize(sdp=request.sdp, type=request.type)

        @pipecat_connection.event_handler("closed")
        async def handle_disconnected(webrtc_connection: SmallWebRTCConnection) -> None:
            logger.info(f"Discarding peer connection for pc_id: {webrtc_connection.pc_id}")
            peer_connections_map.pop(webrtc_connection.pc_id, None)

        # Run pipeline for this connection as tracked asyncio task
        task = asyncio.create_task(run_pipeline(pipecat_connection))
        pipeline_tasks.add(task)
        task.add_done_callback(pipeline_tasks.discard)

    answer = pipecat_connection.get_answer()
    peer_connections_map[answer["pc_id"]] = pipecat_connection

    return WebRTCOfferResponse(**answer)


def main() -> None:
    """Main entry point for the server."""
    # Load settings first so we can use them as defaults
    try:
        settings = Settings()
    except Exception as e:
        print(f"Configuration error: {e}")
        print("Please check your .env file and ensure all required API keys are set.")
        print("See .env.example for reference.")
        raise SystemExit(1) from e

    parser = argparse.ArgumentParser(description="Tambourine Server")
    parser.add_argument(
        "--host",
        default=settings.host,
        help=f"Host to bind to (default: {settings.host})",
    )
    parser.add_argument(
        "--port",
        type=int,
        default=settings.port,
        help=f"Port to listen on (default: {settings.port})",
    )
    parser.add_argument(
        "-v",
        "--verbose",
        action="store_true",
        help="Enable verbose logging",
    )
    args = parser.parse_args()

    # Configure logging
    log_level = "DEBUG" if args.verbose else None
    configure_logging(log_level)

    if args.verbose:
        logger.info("Verbose logging enabled")

    # Initialize services
    if not initialize_services(settings):
        raise SystemExit(1)

    logger.info("=" * 60)
    logger.success("Tambourine Server Ready!")
    logger.info("=" * 60)
    logger.info(f"Server endpoint: http://{args.host}:{args.port}")
    logger.info(f"WebRTC offer endpoint: http://{args.host}:{args.port}/api/offer")
    logger.info(f"Config API endpoint: http://{args.host}:{args.port}/api/*")
    logger.info("Waiting for Tauri client connection...")
    logger.info("Press Ctrl+C to stop")
    logger.info("=" * 60)

    # Run the server
    uvicorn.run(
        app,
        host=args.host,
        port=args.port,
        log_level="warning",
    )


if __name__ == "__main__":
    main()
