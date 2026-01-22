"""Rate limiting utilities for protecting the server from abuse.

This module provides tiered rate limiting that distinguishes between:
1. Registered clients (using official client) - lenient limits
2. Unregistered requests (potential abuse) - strict limits

The key insight is that official clients register via /api/client/register
and include their UUID in subsequent requests. Attackers trying to bypass
the client and spam the server directly will be rate limited.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from loguru import logger
from slowapi import Limiter
from slowapi.util import get_remote_address

if TYPE_CHECKING:
    from starlette.requests import Request


def get_client_identifier(request: Request) -> str:
    """Get a unique identifier for rate limiting based on client registration status.

    For registered clients (with valid UUID), uses IP + UUID which provides
    very high effective limits since each client gets their own bucket.

    For unregistered requests, uses just the IP address for stricter limiting.

    Args:
        request: The incoming request

    Returns:
        A string identifier for rate limiting (IP or IP:UUID)
    """
    ip_address = get_remote_address(request)

    # Try to extract client UUID from request
    # For POST /api/offer, the UUID is in the JSON body under requestData.clientUUID
    # We can't easily access body here without consuming it, so we use a simpler approach:
    # Check if there's a cached UUID on the request state (set by middleware)
    client_uuid = getattr(request.state, "client_uuid", None)

    if client_uuid:
        # Registered client: use IP:UUID for separate rate limit bucket
        # This effectively gives each registered client their own limits
        return f"{ip_address}:{client_uuid}"

    # Unregistered request: use just IP for stricter shared limits
    return ip_address or "unknown"


def get_ip_only(request: Request) -> str:
    """Get just the IP address for rate limiting.

    Used for endpoints like /api/client/register where we want to limit
    registrations per IP regardless of any other factors.

    Args:
        request: The incoming request

    Returns:
        The client's IP address
    """
    return get_remote_address(request) or "unknown"


# Create the limiter with in-memory storage
# Using in-memory is fine for single-server deployments
# For multi-server, would need Redis backend
limiter = Limiter(
    key_func=get_client_identifier,
    default_limits=["100/minute"],  # Default fallback
    storage_uri="memory://",
)


# Rate limit constants
# These are designed so normal client usage will never hit the limits,
# but direct API abuse will be blocked

# Registration: Strict per-IP limit to prevent mass UUID generation
# Normal client registers once, so 10/hour is very generous
RATE_LIMIT_REGISTRATION = "10/hour"

# Client verification: Moderate limit to prevent UUID enumeration
# Normal client verifies once per session, so 30/minute is very generous
RATE_LIMIT_VERIFY = "30/minute"

# WebRTC offer: Lenient for registered clients (they have UUID in key)
# Each client gets their own bucket, so 60/minute per client is fine
# Unregistered attempts share the IP bucket and hit this limit faster
RATE_LIMIT_OFFER = "60/minute"

# ICE candidate patches: Very lenient as these come rapidly during WebRTC setup
# A single connection setup might send 10-20 candidates quickly
RATE_LIMIT_ICE = "200/minute"

# Static config endpoints: Moderate limit
RATE_LIMIT_CONFIG = "60/minute"

# Runtime config endpoints (prompts, stt-timeout): Moderate limit
# These require valid client UUID so abuse is limited
RATE_LIMIT_RUNTIME_CONFIG = "60/minute"

# Providers endpoint: Moderate limit for read-only data
RATE_LIMIT_PROVIDERS = "60/minute"


def log_rate_limit_exceeded(request: Request, limit: str) -> None:
    """Log when a rate limit is exceeded for monitoring.

    Args:
        request: The request that exceeded the limit
        limit: The limit that was exceeded
    """
    ip_address = get_remote_address(request)
    path = request.url.path
    logger.warning(f"Rate limit exceeded: {limit} for IP {ip_address} on {path}")
