"""Client connection management with UUID-based identification.

Manages client registrations and active connections to ensure:
1. One client = one connection (old connections disconnected when same client reconnects)
2. Server tracks clients by persistent UUID
3. Future-compatible with auth system (endpoint becomes auth login)
"""

import asyncio
import contextlib
import uuid
from dataclasses import dataclass, field
from datetime import datetime
from typing import TYPE_CHECKING

from loguru import logger

if TYPE_CHECKING:
    from pipecat.services.ai_services import STTService
    from pipecat.services.llm_service import LLMService
    from pipecat.transports.smallwebrtc.connection import SmallWebRTCConnection

    from processors.context_manager import DictationContextManager
    from processors.turn_controller import TurnController
    from services.provider_registry import LLMProviderId, STTProviderId


@dataclass
class ConnectionInfo:
    """Information about an active client connection."""

    client_uuid: str
    connection: "SmallWebRTCConnection"
    pipeline_task: asyncio.Task[None]
    connected_at: datetime = field(default_factory=datetime.now)
    # Pipeline component references for HTTP API configuration
    context_manager: "DictationContextManager | None" = None
    turn_controller: "TurnController | None" = None
    stt_services: "dict[STTProviderId, STTService] | None" = None
    llm_services: "dict[LLMProviderId, LLMService] | None" = None


class ClientConnectionManager:
    """Manages client UUIDs and active connections (in-memory only).

    UUIDs are stored in-memory only. After server restart, clients receive 401
    and re-register automatically. This is by design - future auth will add
    persistent storage.
    """

    def __init__(self) -> None:
        """Initialize the client connection manager."""
        self._registered_uuids: set[str] = set()
        self._connections: dict[str, ConnectionInfo] = {}

    def generate_and_register_uuid(self) -> str:
        """Generate a new UUID and register it.

        Returns:
            The newly generated and registered UUID string.
        """
        new_uuid = str(uuid.uuid4())
        self._registered_uuids.add(new_uuid)
        logger.debug(f"Generated and registered new UUID: {new_uuid}")
        return new_uuid

    def is_registered(self, client_uuid: str) -> bool:
        """Check if a UUID is registered.

        Args:
            client_uuid: The UUID to check.

        Returns:
            True if the UUID is registered, False otherwise.
        """
        return client_uuid in self._registered_uuids

    def register_connection(
        self,
        client_uuid: str,
        connection: "SmallWebRTCConnection",
        pipeline_task: asyncio.Task[None],
        *,
        context_manager: "DictationContextManager | None" = None,
        turn_controller: "TurnController | None" = None,
        stt_services: "dict[STTProviderId, STTService] | None" = None,
        llm_services: "dict[LLMProviderId, LLMService] | None" = None,
    ) -> None:
        """Register an active connection for a client UUID.

        Args:
            client_uuid: The client's UUID.
            connection: The WebRTC connection.
            pipeline_task: The pipeline task associated with this connection.
            context_manager: The DictationContextManager for this connection.
            turn_controller: The TurnController for this connection.
            stt_services: Dictionary mapping STT provider IDs to services.
            llm_services: Dictionary mapping LLM provider IDs to services.
        """
        self._connections[client_uuid] = ConnectionInfo(
            client_uuid=client_uuid,
            connection=connection,
            pipeline_task=pipeline_task,
            context_manager=context_manager,
            turn_controller=turn_controller,
            stt_services=stt_services,
            llm_services=llm_services,
        )
        logger.debug(f"Registered connection for client: {client_uuid}")

    def unregister_connection(self, client_uuid: str) -> None:
        """Unregister a connection for a client UUID.

        Args:
            client_uuid: The client's UUID to unregister.
        """
        if client_uuid in self._connections:
            del self._connections[client_uuid]
            logger.debug(f"Unregistered connection for client: {client_uuid}")

    async def disconnect_existing(self, client_uuid: str) -> None:
        """Disconnect any existing connection with the same UUID.

        This ensures one client = one connection. When a client reconnects,
        the old connection is terminated.

        Args:
            client_uuid: The client's UUID whose existing connection should be closed.
        """
        if client_uuid not in self._connections:
            return

        existing = self._connections[client_uuid]
        logger.info(f"Disconnecting existing connection for client: {client_uuid}")

        # Cancel the pipeline task - this will trigger cleanup
        if not existing.pipeline_task.done():
            with contextlib.suppress(asyncio.CancelledError):
                await existing.pipeline_task
                pass

        # Close the WebRTC connection
        try:
            await existing.connection.disconnect()
        except Exception as error:
            logger.warning(f"Error closing existing connection: {error}")

        # Remove from our tracking
        self.unregister_connection(client_uuid)

    def get_active_connection_count(self) -> int:
        """Get the number of active connections.

        Returns:
            The count of active connections.
        """
        return len(self._connections)

    def get_registered_uuid_count(self) -> int:
        """Get the number of registered UUIDs.

        Returns:
            The count of registered UUIDs.
        """
        return len(self._registered_uuids)

    def get_connection(self, client_uuid: str) -> ConnectionInfo | None:
        """Get the connection info for a client UUID.

        Args:
            client_uuid: The client's UUID.

        Returns:
            The ConnectionInfo if the client is connected, None otherwise.
        """
        return self._connections.get(client_uuid)
