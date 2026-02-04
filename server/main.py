#!/usr/bin/env python3
"""Tambourine Server - SmallWebRTC-based Pipecat Server.

A FastAPI server that receives audio from a Tauri client via WebRTC,
processes it through STT and LLM formatting, and returns formatted text.

Usage:
    python main.py
    python main.py --port 8765
"""

import asyncio
import re
from contextlib import asynccontextmanager
from dataclasses import dataclass
from typing import Annotated, Any, Final, cast

import typer
import uvicorn
from fastapi import FastAPI, HTTPException, Request
from fastapi.middleware.cors import CORSMiddleware
from fastapi.responses import JSONResponse
from loguru import logger
from pipecat.audio.vad.silero import SileroVADAnalyzer
from pipecat.frames.frames import HeartbeatFrame
from pipecat.observers.loggers.user_bot_latency_log_observer import UserBotLatencyLogObserver
from pipecat.pipeline.llm_switcher import LLMSwitcher
from pipecat.pipeline.pipeline import Pipeline
from pipecat.pipeline.runner import PipelineRunner
from pipecat.pipeline.service_switcher import ServiceSwitcher, ServiceSwitcherStrategyManual
from pipecat.pipeline.task import PipelineParams, PipelineTask
from pipecat.processors.frameworks.rtvi import RTVIProcessor
from pipecat.services.llm_service import LLMService
from pipecat.services.stt_service import STTService
from pipecat.transports.base_transport import TransportParams
from pipecat.transports.smallwebrtc.connection import IceServer, SmallWebRTCConnection
from pipecat.transports.smallwebrtc.request_handler import (
    SmallWebRTCPatchRequest,
    SmallWebRTCRequest,
    SmallWebRTCRequestHandler,
)
from pipecat.transports.smallwebrtc.transport import SmallWebRTCTransport
from slowapi import _rate_limit_exceeded_handler
from slowapi.errors import RateLimitExceeded

from api.config_api import config_router
from config.settings import Settings
from processors.client_manager import ClientConnectionManager
from processors.configuration import ConfigurationHandler
from processors.context_manager import DictationContextManager
from processors.llm_gate import LLMGateFilter
from processors.turn_controller import TurnController
from protocol.messages import (
    SetLLMProviderMessage,
    SetSTTProviderMessage,
    StartRecordingMessage,
    StopRecordingMessage,
    UnknownClientMessage,
    parse_client_message,
)
from services.providers import (
    LLMProviderId,
    STTProviderId,
    create_all_available_llm_services,
    create_all_available_stt_services,
    get_available_llm_providers,
    get_available_stt_providers,
)
from utils.logger import configure_logging
from utils.observers import PipelineLogObserver
from utils.rate_limiter import (
    RATE_LIMIT_ICE,
    RATE_LIMIT_OFFER,
    RATE_LIMIT_REGISTRATION,
    RATE_LIMIT_VERIFY,
    get_ip_only,
    limiter,
)

# ICE servers for WebRTC NAT traversal
ICE_SERVERS: Final[list[IceServer]] = [
    IceServer(urls="stun:stun.l.google.com:19302"),
]

# Pattern to match mDNS ICE candidates in SDP (e.g., "abc123-def4.local")
# These candidates only work for local network peers and cause aioice state
# issues when resolution fails on cloud deployments.
MDNS_CANDIDATE_PATTERN: Final[re.Pattern[str]] = re.compile(
    r"^a=candidate:.*\s[a-f0-9-]+\.local\s.*$",
    re.MULTILINE | re.IGNORECASE,
)

# Set to hold background tasks to prevent garbage collection before completion
_background_tasks: set[asyncio.Task[Any]] = set()


def create_background_task(coro: Any) -> asyncio.Task[Any]:
    """Create a background task that won't be garbage collected before completion."""
    task = asyncio.create_task(coro)
    _background_tasks.add(task)
    task.add_done_callback(_background_tasks.discard)
    return task


def filter_mdns_candidates_from_sdp(sdp: str) -> str:
    """Remove mDNS ICE candidates from SDP to prevent aioice resolution issues.

    mDNS candidates (*.local addresses) are sent for privacy, but these
    cannot be resolved on cloud servers (different network). The aioice library
    accumulates stale state when mDNS resolution fails, causing subsequent
    connections to fail with 'NoneType' has no attribute 'sendto'.

    Filtering these candidates is safe because:
    1. mDNS only works on local networks (same broadcast domain)
    2. Client-to-cloud connections use srflx (STUN) candidates instead
    3. Connection still works via server-reflexive candidates

    Args:
        sdp: The original SDP string from the client

    Returns:
        SDP with mDNS candidates removed
    """
    filtered_sdp = MDNS_CANDIDATE_PATTERN.sub("", sdp)
    # Clean up any resulting blank lines
    filtered_sdp = re.sub(r"\n{3,}", "\n\n", filtered_sdp)
    return filtered_sdp


def is_mdns_candidate(candidate: str) -> bool:
    """Check if an ICE candidate string contains an mDNS address (.local).

    mDNS candidates use UUIDs like: a8b3c4d5-e6f7-8901-2345-6789abcdef01.local
    These only work on local networks and cause aioice state issues when
    resolution fails on cloud servers.

    Args:
        candidate: The ICE candidate string (SDP a=candidate line content)

    Returns:
        True if this is an mDNS candidate, False otherwise
    """
    return bool(re.search(r"\s[a-f0-9-]+\.local\s", candidate, re.IGNORECASE))


@dataclass
class AppServices:
    """Container for application services, stored on app.state.

    Note: STT and LLM services are created per-connection in run_pipeline()
    to ensure complete isolation between concurrent clients. Each client
    gets fresh service instances with independent WebSocket connections.

    The available_stt_providers and available_llm_providers lists are
    pre-computed at startup since Settings is immutable after initialization.
    """

    settings: Settings
    webrtc_handler: SmallWebRTCRequestHandler
    active_pipeline_tasks: set[asyncio.Task[None]]
    client_manager: ClientConnectionManager
    available_stt_providers: list[STTProviderId]
    available_llm_providers: list[LLMProviderId]


async def run_pipeline(
    webrtc_connection: SmallWebRTCConnection,
    services: AppServices,
    *,
    stt_services: dict[STTProviderId, STTService],
    llm_services: dict[LLMProviderId, LLMService],
    context_manager: DictationContextManager,
    turn_controller: TurnController,
    llm_gate: LLMGateFilter,
) -> None:
    """Run the Pipecat pipeline for a single WebRTC connection.

    Args:
        webrtc_connection: The SmallWebRTCConnection instance for this client
        services: Application services container
        stt_services: Pre-created STT services for this connection
        llm_services: Pre-created LLM services for this connection
        context_manager: Pre-created context manager for this connection
        turn_controller: Pre-created turn controller for this connection
    """
    logger.info("Starting pipeline for new WebRTC connection")

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

    # Build pipeline - Pipecat 0.0.101+ handles RTVI automatically via task.rtvi
    # The aggregator pair from context_manager collects transcriptions and LLM responses
    pipeline = Pipeline(
        [
            transport.input(),
            stt_switcher,
            turn_controller,  # Controls turn boundaries, passes transcriptions through
            llm_gate,  # Gates frames to aggregator based on LLM formatting setting
            context_manager.user_aggregator(),  # Collects transcriptions, emits LLMContextFrame
            llm_switcher,
            context_manager.assistant_aggregator(),  # Collects LLM responses
            transport.output(),
        ]
    )

    # Create pipeline task - RTVI is automatically enabled and accessible via task.rtvi
    # This avoids duplicate RTVIObservers that caused text duplication in 0.0.101
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
            PipelineLogObserver(),
        ],
    )

    # ConfigurationHandler processes provider switching messages from RTVI client
    # Note: State-only config (prompts, timeouts) is now handled via HTTP API
    config_handler = ConfigurationHandler(
        rtvi_processor=task.rtvi,
        stt_switcher=stt_switcher,
        llm_switcher=llm_switcher,
        stt_services=stt_services,
        llm_services=llm_services,
        settings=services.settings,
    )

    # Register event handler for client messages on the RTVI processor
    @task.rtvi.event_handler("on_client_message")
    async def on_client_message(processor: RTVIProcessor, message: Any) -> None:
        """Handle RTVI client messages for configuration and recording control."""
        _ = processor  # Unused, required by event handler signature

        # Parse the raw RTVI message into a typed Pydantic model
        # This converts the message.type + message.data structure into a discriminated union
        raw_data = {
            "type": message.type if hasattr(message, "type") else None,
            "data": message.data if hasattr(message, "data") else {},
        }
        if raw_data["type"] is None:
            return

        # Use forward-compatible parser (never returns None)
        parsed = parse_client_message(raw_data)

        # Handle the typed message with exhaustive pattern matching
        match parsed:
            case StartRecordingMessage():
                llm_gate.reset_for_recording()
                await context_manager.reset_aggregator()
                await turn_controller.start_recording()
            case StopRecordingMessage():
                await turn_controller.stop_recording()
            case SetSTTProviderMessage() | SetLLMProviderMessage():
                await config_handler.handle_config_message(parsed)
            case UnknownClientMessage():
                pass  # Already logged at debug level in parse_client_message

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
        client_manager=ClientConnectionManager(),
        available_stt_providers=available_stt,
        available_llm_providers=available_llm,
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

# Add rate limiter to app state
app.state.limiter = limiter
app.add_exception_handler(RateLimitExceeded, _rate_limit_exceeded_handler)  # type: ignore[arg-type]

app.add_middleware(
    CORSMiddleware,  # type: ignore[invalid-argument-type]
    allow_origins=["*"],
    allow_methods=["*"],
    allow_headers=["*"],
)


# Global exception handler to ensure CORS headers are included in error responses.
# FastAPI's CORSMiddleware may not add headers to unhandled exception responses,
# causing misleading "CORS errors".
@app.exception_handler(Exception)
async def global_exception_handler(request: Request, exc: Exception) -> JSONResponse:
    """Ensure CORS headers are included even in error responses."""
    logger.error(f"Unhandled exception: {exc}")
    return JSONResponse(
        status_code=500,
        content={"detail": str(exc)},
        headers={
            "Access-Control-Allow-Origin": "*",
            "Access-Control-Allow-Methods": "*",
            "Access-Control-Allow-Headers": "*",
        },
    )


# Include config routes
app.include_router(config_router)


@app.get("/health")
async def health_check() -> dict[str, str]:
    """Health check endpoint for container orchestration (e.g., Lightsail)."""
    return {"status": "ok"}


# =============================================================================
# Client Registration Endpoints
# =============================================================================


@app.post("/api/client/register")
@limiter.limit(RATE_LIMIT_REGISTRATION, key_func=get_ip_only)
async def register_client(request: Request) -> dict[str, str]:
    """Generate, register, and return a new client UUID.

    This endpoint is called by clients on first connection or when their
    stored UUID is rejected (e.g., after server restart).

    Rate limited by IP to prevent mass UUID registration attacks.

    Returns:
        A dictionary containing the newly generated UUID.
    """
    services: AppServices = request.app.state.services
    client_uuid = services.client_manager.generate_and_register_uuid()
    logger.success(f"Registered new client: {client_uuid}")
    return {"uuid": client_uuid}


@app.get("/api/client/verify/{client_uuid}")
@limiter.limit(RATE_LIMIT_VERIFY, key_func=get_ip_only)
async def verify_client(client_uuid: str, request: Request) -> dict[str, bool]:
    """Verify if a client UUID is registered with the server.

    This endpoint allows clients to check if their stored UUID is still valid
    (e.g., after server restart where in-memory registrations are lost).

    Rate limited by IP to prevent UUID enumeration attacks.

    Returns:
        A dictionary with 'registered' boolean indicating if UUID is valid.
    """
    services: AppServices = request.app.state.services
    is_registered = services.client_manager.is_registered(client_uuid)
    return {"registered": is_registered}


# =============================================================================
# WebRTC Endpoints
# =============================================================================


@app.post("/api/offer")
@limiter.limit(RATE_LIMIT_OFFER, key_func=get_ip_only)
async def webrtc_offer(
    request: Request,
) -> dict[str, str] | None:
    """Handle WebRTC offer from client using SmallWebRTCRequestHandler.

    This endpoint handles the WebRTC signaling handshake:
    1. Receives SDP offer from client (filtering mDNS candidates)
    2. Validates client UUID from request_data (rejects unregistered UUIDs)
    3. Disconnects any existing connection with the same UUID
    4. Creates or reuses a SmallWebRTCConnection via the handler
    5. Returns SDP answer to client
    6. Spawns the Pipecat pipeline as a background task

    Rate limited by IP. Normal client usage (reconnecting occasionally) won't
    hit the limit, but attackers spamming connection attempts will be blocked.
    """
    services: AppServices = request.app.state.services

    # Parse request body using from_dict to handle camelCase requestData field
    # FastAPI's auto-parsing doesn't use the classmethod that handles the conversion
    request_body = await request.json()
    webrtc_request = SmallWebRTCRequest.from_dict(request_body)

    # Extract client UUID from request_data
    client_uuid: str | None = None
    if webrtc_request.request_data:
        client_uuid = webrtc_request.request_data.get("clientUUID")
    logger.info(f"Incoming client UUID: {client_uuid}")

    # Require UUID - clients must register first
    if not client_uuid:
        logger.warning("Rejected connection without client UUID")
        raise HTTPException(
            status_code=401,
            detail="Client UUID required. Please register first.",
        )

    # Validate UUID is registered
    if not services.client_manager.is_registered(client_uuid):
        logger.warning(f"Rejected unregistered client UUID: {client_uuid}")
        raise HTTPException(
            status_code=401,
            detail="Unregistered client UUID. Please register first.",
        )

    # Handle existing connection with same UUID (one client = one connection)
    # 1. Synchronously remove old connection from tracking (frees UUID slot immediately)
    # 2. Clean up old connection in background (non-blocking)
    # This avoids the race condition where background cleanup accidentally kills new connection
    old_connection = services.client_manager.take_existing_connection(client_uuid)
    if old_connection:
        create_background_task(services.client_manager.cleanup_connection(old_connection))
    logger.info(f"Client connecting with UUID: {client_uuid}")

    # Filter mDNS candidates from SDP to prevent aioice resolution issues.
    # See filter_mdns_candidates_from_sdp() docstring for details.
    filtered_sdp = filter_mdns_candidates_from_sdp(webrtc_request.sdp)
    if filtered_sdp != webrtc_request.sdp:
        logger.info("Filtered mDNS candidates from SDP offer")
        webrtc_request = SmallWebRTCRequest(
            sdp=filtered_sdp,
            type=webrtc_request.type,
            pc_id=webrtc_request.pc_id,
            restart_pc=webrtc_request.restart_pc,
            request_data=webrtc_request.request_data,
        )

    async def connection_callback(connection: SmallWebRTCConnection) -> None:
        """Callback invoked when connection is ready - spawns the pipeline."""
        # Create fresh service instances for this connection to ensure isolation
        # between concurrent clients. Each client gets independent WebSocket
        # connections to STT/LLM providers.
        # Uses pre-computed provider lists from AppServices to avoid redundant
        # iteration through all providers on every connection.
        stt_services = create_all_available_stt_services(
            services.settings,
            services.available_stt_providers,
        )
        llm_services = create_all_available_llm_services(
            services.settings,
            services.available_llm_providers,
        )

        # Create pipeline processors
        # DictationContextManager wraps LLMContextAggregatorPair with dictation-specific features
        context_manager = DictationContextManager()
        turn_controller = TurnController()
        llm_gate = LLMGateFilter()
        # Wire up turn controller to context manager for context reset coordination
        turn_controller.set_context_manager(context_manager)

        task = asyncio.create_task(
            run_pipeline(
                connection,
                services,
                stt_services=stt_services,
                llm_services=llm_services,
                context_manager=context_manager,
                turn_controller=turn_controller,
                llm_gate=llm_gate,
            )
        )
        services.active_pipeline_tasks.add(task)
        task.add_done_callback(services.active_pipeline_tasks.discard)

        # Track connection by UUID with component references for HTTP API access
        services.client_manager.register_connection(
            client_uuid,
            connection,
            task,
            context_manager=context_manager,
            turn_controller=turn_controller,
            llm_gate=llm_gate,
            stt_services=stt_services,
            llm_services=llm_services,
        )

    answer = await services.webrtc_handler.handle_web_request(
        request=webrtc_request,
        webrtc_connection_callback=connection_callback,
    )

    return answer


@app.patch("/api/offer")
@limiter.limit(RATE_LIMIT_ICE, key_func=get_ip_only)
async def webrtc_ice_candidate(
    patch_request: SmallWebRTCPatchRequest,
    request: Request,
) -> dict[str, str]:
    """Handle ICE candidate patches for WebRTC connections.

    Filters mDNS ICE candidates sent via ICE trickle to prevent aioice
    resolution issues. mDNS candidates (.local addresses) are sent
    for privacy, but these cause state accumulation issues in aioice.

    Rate limited with a high threshold as ICE candidates come in rapid
    bursts during WebRTC connection setup.
    """
    services: AppServices = request.app.state.services

    # Filter out mDNS candidates to prevent aioice resolution issues
    # macOS WebKit sends mDNS candidates via ICE trickle (not in SDP offer)
    if patch_request.candidates:
        original_count = len(patch_request.candidates)
        filtered_candidates = [
            c for c in patch_request.candidates if not is_mdns_candidate(c.candidate)
        ]
        filtered_count = original_count - len(filtered_candidates)

        if filtered_count > 0:
            logger.info(f"Filtered {filtered_count} mDNS ICE candidates from trickle")
            patch_request = SmallWebRTCPatchRequest(
                pc_id=patch_request.pc_id,
                candidates=filtered_candidates,
            )

    # Only process if we have candidates remaining after filtering
    if patch_request.candidates:
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
        logger.debug("Verbose logging enabled")

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
