#!/usr/bin/env python3
"""Tambourine Server - SmallWebRTC-based Pipecat Server.

A FastAPI server that receives audio from a Tauri client via WebRTC,
processes it through STT and LLM formatting, and returns formatted text.

Usage:
    python main.py
    python main.py --port 8765
"""

import asyncio
from contextlib import asynccontextmanager
from dataclasses import dataclass
from typing import Annotated, Any, Final, cast

import typer
import uvicorn
from fastapi import FastAPI, Request
from fastapi.middleware.cors import CORSMiddleware
from loguru import logger
from pipecat.audio.vad.silero import SileroVADAnalyzer
from pipecat.frames.frames import HeartbeatFrame
from pipecat.observers.loggers.user_bot_latency_log_observer import UserBotLatencyLogObserver
from pipecat.pipeline.llm_switcher import LLMSwitcher
from pipecat.pipeline.pipeline import Pipeline
from pipecat.pipeline.runner import PipelineRunner
from pipecat.pipeline.service_switcher import ServiceSwitcher, ServiceSwitcherStrategyManual
from pipecat.pipeline.task import PipelineParams, PipelineTask
from pipecat.processors.frameworks.rtvi import RTVIObserver, RTVIProcessor
from pipecat.transports.base_transport import TransportParams
from pipecat.transports.smallwebrtc.connection import IceServer, SmallWebRTCConnection
from pipecat.transports.smallwebrtc.request_handler import (
    SmallWebRTCPatchRequest,
    SmallWebRTCRequest,
    SmallWebRTCRequestHandler,
)
from pipecat.transports.smallwebrtc.transport import SmallWebRTCTransport

from api.config_server import config_router
from config.settings import Settings
from processors.configuration import ConfigurationHandler
from processors.llm import TranscriptionToLLMConverter
from processors.transcription_buffer import TranscriptionBufferProcessor
from services.providers import (
    create_all_available_llm_services,
    create_all_available_stt_services,
    get_available_llm_providers,
    get_available_stt_providers,
)
from utils.logger import configure_logging
from utils.observers import PipelineLogObserver

# ICE servers for WebRTC NAT traversal
ICE_SERVERS: Final[list[IceServer]] = [
    IceServer(urls="stun:stun.l.google.com:19302"),
]


@dataclass
class AppServices:
    """Container for application services, stored on app.state.

    Note: STT and LLM services are created per-connection in run_pipeline()
    to ensure complete isolation between concurrent clients. Each client
    gets fresh service instances with independent WebSocket connections.
    """

    settings: Settings
    webrtc_handler: SmallWebRTCRequestHandler
    active_pipeline_tasks: set[asyncio.Task[None]]


async def run_pipeline(
    webrtc_connection: SmallWebRTCConnection,
    services: AppServices,
) -> None:
    """Run the Pipecat pipeline for a single WebRTC connection.

    Args:
        webrtc_connection: The SmallWebRTCConnection instance for this client
        services: Application services container
    """
    logger.info("Starting pipeline for new WebRTC connection")

    # Create fresh service instances for this connection to ensure isolation
    # between concurrent clients. Each client gets independent WebSocket
    # connections to STT/LLM providers.
    stt_services = create_all_available_stt_services(services.settings)
    llm_services = create_all_available_llm_services(services.settings)

    # Create transport using the WebRTC connection
    # (client connects with enableMic: false, only enables when recording starts)
    transport = SmallWebRTCTransport(
        webrtc_connection=webrtc_connection,
        params=TransportParams(
            audio_in_enabled=True,
            audio_out_enabled=False,  # No audio output for dictation
            vad_analyzer=SileroVADAnalyzer(),
        ),
    )

    # Create service switchers for this connection
    from pipecat.pipeline.base_pipeline import FrameProcessor as PipecatFrameProcessor

    stt_service_list = cast(list[PipecatFrameProcessor], list(stt_services.values()))
    llm_service_list = list(llm_services.values())

    stt_switcher = ServiceSwitcher(
        services=stt_service_list,
        strategy_type=ServiceSwitcherStrategyManual,
    )

    llm_switcher = LLMSwitcher(
        llms=llm_service_list,
        strategy_type=ServiceSwitcherStrategyManual,
    )

    # Initialize processors
    transcription_to_llm = TranscriptionToLLMConverter()
    transcription_buffer = TranscriptionBufferProcessor()

    # RTVIProcessor handles the RTVI protocol (client messages, server responses)
    rtvi_processor = RTVIProcessor()

    # ConfigurationHandler processes config messages from RTVI client messages
    config_handler = ConfigurationHandler(
        rtvi_processor=rtvi_processor,
        stt_switcher=stt_switcher,
        llm_switcher=llm_switcher,
        llm_converter=transcription_to_llm,
        transcription_buffer=transcription_buffer,
        stt_services=stt_services,
        llm_services=llm_services,
    )

    # Register event handler for client messages
    @rtvi_processor.event_handler("on_client_message")
    async def on_client_message(processor: RTVIProcessor, message: Any) -> None:
        """Handle RTVI client messages for configuration and recording control."""
        _ = processor  # Unused, required by event handler signature

        # Extract message type and data from RTVI client message
        msg_type = message.type if hasattr(message, "type") else None
        data = message.data if hasattr(message, "data") else {}
        if not msg_type:
            return

        # Handle recording control messages
        if msg_type == "start-recording":
            await transcription_buffer.start_recording()
            return
        if msg_type == "stop-recording":
            await transcription_buffer.stop_recording()
            return

        # Handle configuration messages
        await config_handler.handle_client_message(msg_type, data)

    # Build pipeline - RTVIProcessor at the start handles RTVI protocol
    pipeline = Pipeline(
        [
            transport.input(),
            rtvi_processor,  # Handles RTVI protocol messages
            stt_switcher,
            transcription_buffer,
            transcription_to_llm,
            llm_switcher,
            transport.output(),
        ]
    )

    # Create pipeline task with RTVIObserver to send bot-llm-text to client
    task = PipelineTask(
        pipeline,
        params=PipelineParams(
            allow_interruptions=False,
            enable_metrics=True,
            enable_usage_metrics=True,
            enable_heartbeats=True,
        ),
        idle_timeout_frames=(HeartbeatFrame,),
        observers=[
            UserBotLatencyLogObserver(),
            RTVIObserver(rtvi_processor),  # Sends bot-llm-text messages to client
            PipelineLogObserver(),
        ],
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


def initialize_services(settings: Settings) -> AppServices | None:
    """Initialize application services container.

    Validates that at least one STT and LLM provider is available.
    Actual service instances are created per-connection in run_pipeline()
    to ensure complete isolation between concurrent clients.

    Args:
        settings: Application settings

    Returns:
        AppServices instance if successful, None otherwise
    """
    available_stt = get_available_stt_providers(settings)
    available_llm = get_available_llm_providers(settings)

    if not available_stt:
        logger.error("No STT providers available. Configure at least one STT API key.")
        return None

    if not available_llm:
        logger.error("No LLM providers available. Configure at least one LLM API key.")
        return None

    logger.info(f"Available STT providers: {[p.value for p in available_stt]}")
    logger.info(f"Available LLM providers: {[p.value for p in available_llm]}")

    return AppServices(
        settings=settings,
        webrtc_handler=SmallWebRTCRequestHandler(ice_servers=ICE_SERVERS),
        active_pipeline_tasks=set(),
    )


@asynccontextmanager
async def lifespan(fastapi_app: FastAPI):  # noqa: ANN201
    """FastAPI lifespan context manager for cleanup."""
    yield
    logger.info("Shutting down server...")

    # Get services from app state (may not exist if startup failed)
    services: AppServices | None = getattr(fastapi_app.state, "services", None)
    if services is None:
        logger.warning("Services not initialized, skipping cleanup")
        return

    # Cancel all active pipeline tasks for graceful shutdown
    if services.active_pipeline_tasks:
        logger.info(f"Cancelling {len(services.active_pipeline_tasks)} active pipeline tasks...")
        for task in list(services.active_pipeline_tasks):
            task.cancel()
        # Wait for all tasks to complete with timeout to avoid hanging
        try:
            async with asyncio.timeout(5.0):
                await asyncio.gather(*services.active_pipeline_tasks, return_exceptions=True)
            logger.info("All pipeline tasks cancelled")
        except TimeoutError:
            logger.warning("Timeout waiting for pipeline tasks to cancel")

    # SmallWebRTCRequestHandler manages all connections - close them cleanly
    await services.webrtc_handler.close()
    logger.success("All connections cleaned up")


# Create FastAPI app
app = FastAPI(title="Tambourine Server", lifespan=lifespan)

# CORS for Tauri frontend
app.add_middleware(
    CORSMiddleware,  # type: ignore[invalid-argument-type]
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)

# Include config routes
app.include_router(config_router)


@app.get("/health")
async def health_check() -> dict[str, str]:
    """Health check endpoint for container orchestration (e.g., Lightsail)."""
    return {"status": "ok"}


# =============================================================================
# WebRTC Endpoints
# =============================================================================


@app.post("/api/offer")
async def webrtc_offer(
    webrtc_request: SmallWebRTCRequest,
    request: Request,
) -> dict[str, str] | None:
    """Handle WebRTC offer from client using SmallWebRTCRequestHandler.

    This endpoint handles the WebRTC signaling handshake:
    1. Receives SDP offer from client
    2. Creates or reuses a SmallWebRTCConnection via the handler
    3. Returns SDP answer to client
    4. Spawns the Pipecat pipeline as a background task
    """
    services: AppServices = request.app.state.services

    async def connection_callback(connection: SmallWebRTCConnection) -> None:
        """Callback invoked when connection is ready - spawns the pipeline."""
        task = asyncio.create_task(run_pipeline(connection, services))
        services.active_pipeline_tasks.add(task)
        task.add_done_callback(services.active_pipeline_tasks.discard)

    answer = await services.webrtc_handler.handle_web_request(
        request=webrtc_request,
        webrtc_connection_callback=connection_callback,
    )

    return answer


@app.patch("/api/offer")
async def webrtc_ice_candidate(
    patch_request: SmallWebRTCPatchRequest,
    request: Request,
) -> dict[str, str]:
    """Handle ICE candidate patches for WebRTC connections."""
    services: AppServices = request.app.state.services
    await services.webrtc_handler.handle_patch_request(patch_request)
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

    # Initialize services and store on app.state
    services = initialize_services(settings)
    if services is None:
        raise SystemExit(1)
    app.state.services = services

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
