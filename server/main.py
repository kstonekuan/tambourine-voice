#!/usr/bin/env python3
"""Tambourine Server - SmallWebRTC-based Pipecat Server.

A FastAPI server that receives audio from a Tauri client via WebRTC,
processes it through STT and LLM cleanup, and returns cleaned text.

Usage:
    python main.py
    python main.py --port 8765
"""

from contextlib import asynccontextmanager
from typing import Annotated, Any, cast

import typer
import uvicorn
from fastapi import BackgroundTasks, FastAPI
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
from pipecat.transports.smallwebrtc.request_handler import (
    SmallWebRTCPatchRequest,
    SmallWebRTCRequest,
    SmallWebRTCRequestHandler,
)
from pipecat.transports.smallwebrtc.transport import SmallWebRTCTransport
from pydantic import BaseModel

from api.config_server import config_router, set_available_providers
from config.settings import Settings
from processors.configuration import ConfigurationProcessor
from processors.llm_cleanup import LLMResponseToRTVIConverter, TranscriptionToLLMConverter
from processors.transcription_buffer import TranscriptionBufferProcessor
from services.providers import (
    LLMProviderId,
    STTProviderId,
    create_all_available_llm_services,
    create_all_available_stt_services,
)
from utils.logger import configure_logging

# ICE servers for WebRTC NAT traversal
ice_servers = [
    IceServer(urls="stun:stun.l.google.com:19302"),
]

# SmallWebRTC request handler - manages connection lifecycle
small_webrtc_handler = SmallWebRTCRequestHandler(ice_servers=ice_servers)

# Shared state for service instances (created once at startup)
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
            # Log when pipeline is fully started (StartFrame has passed through all processors)
            logger.success("Pipeline fully started (StartFrame passed through all processors)")
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
    # audio_in_stream_on_start=False prevents timeout errors when mic is disabled
    # (client connects with enableMic: false, only enables when recording starts)
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

    # Configuration processor handles runtime config via data channel
    # (replaces global state access from REST endpoints)
    config_processor = ConfigurationProcessor(
        stt_switcher=stt_switcher,
        llm_switcher=llm_switcher,
        llm_converter=transcription_to_llm,
        transcription_buffer=transcription_buffer,
        stt_services=_stt_services,
        llm_services=_llm_services,
    )

    llm_response_converter = LLMResponseToRTVIConverter()
    text_response = TextResponseProcessor()

    # Build pipeline
    pipeline = Pipeline(
        [
            transport.input(),
            config_processor,  # Handles config messages from data channel
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

    # Set available providers for REST API endpoint
    set_available_providers(_stt_services, _llm_services)

    return True


@asynccontextmanager
async def lifespan(_fastapi_app: FastAPI):  # noqa: ANN201
    """FastAPI lifespan context manager for cleanup."""
    yield
    logger.info("Shutting down server...")
    # SmallWebRTCRequestHandler manages all connections - close them cleanly
    await small_webrtc_handler.close()
    logger.success("All connections cleaned up")


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
# WebRTC Endpoints
# =============================================================================


@app.post("/api/offer")
async def webrtc_offer(
    request: SmallWebRTCRequest,
    background_tasks: BackgroundTasks,
) -> dict[str, Any]:
    """Handle WebRTC offer from client using SmallWebRTCRequestHandler.

    This endpoint handles the WebRTC signaling handshake:
    1. Receives SDP offer from client
    2. Creates or reuses a SmallWebRTCConnection via the handler
    3. Returns SDP answer to client
    4. Spawns the Pipecat pipeline as a background task
    """

    async def connection_callback(connection: SmallWebRTCConnection) -> None:
        """Callback invoked when connection is ready - spawns the pipeline."""
        background_tasks.add_task(run_pipeline, connection)

    answer = await small_webrtc_handler.handle_web_request(
        request=request,
        webrtc_connection_callback=connection_callback,
    )
    # handler.handle_web_request always returns a dict with SDP answer
    return answer  # type: ignore


@app.patch("/api/offer")
async def webrtc_ice_candidate(request: SmallWebRTCPatchRequest) -> dict[str, str]:
    """Handle ICE candidate patches for WebRTC connections."""
    await small_webrtc_handler.handle_patch_request(request)
    return {"status": "success"}


def main(
    host: Annotated[str | None, typer.Option(help="Host to bind to")] = None,
    port: Annotated[int | None, typer.Option(help="Port to listen on")] = None,
    verbose: Annotated[
        bool, typer.Option("-v", "--verbose", help="Enable verbose logging")
    ] = False,
) -> None:
    """Tambourine Server - Voice dictation with AI cleanup."""
    # Load settings first so we can use them as defaults
    try:
        settings = Settings()
    except Exception as e:
        print(f"Configuration error: {e}")
        print("Please check your .env file and ensure all required API keys are set.")
        print("See .env.example for reference.")
        raise SystemExit(1) from e

    # Use settings defaults if not provided via CLI
    effective_host = host or settings.host
    effective_port = port or settings.port

    # Configure logging
    log_level = "DEBUG" if verbose else None
    configure_logging(log_level)

    if verbose:
        logger.info("Verbose logging enabled")

    # Initialize services
    if not initialize_services(settings):
        raise SystemExit(1)

    logger.info("=" * 60)
    logger.success("Tambourine Server Ready!")
    logger.info("=" * 60)
    logger.info(f"Server endpoint: http://{effective_host}:{effective_port}")
    logger.info(f"WebRTC offer endpoint: http://{effective_host}:{effective_port}/api/offer")
    logger.info(f"Config API endpoint: http://{effective_host}:{effective_port}/api/*")
    logger.info("Waiting for Tauri client connection...")
    logger.info("Press Ctrl+C to stop")
    logger.info("=" * 60)

    # Run the server
    uvicorn.run(
        app,
        host=effective_host,
        port=effective_port,
        log_level="warning",
    )


if __name__ == "__main__":
    typer.run(main)
