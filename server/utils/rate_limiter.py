"""Rate limiting utilities for protecting the server from abuse.

This module provides IP-based rate limiting to prevent API abuse.
Each endpoint has configurable limits appropriate for its expected usage pattern.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from slowapi import Limiter
from slowapi.util import get_remote_address

if TYPE_CHECKING:
    from starlette.requests import Request


def get_ip_only(request: Request) -> str:
    """Get the client's IP address for rate limiting.

    Args:
        request: The incoming request

    Returns:
        The client's IP address, or "unknown" if not available
    """
    return get_remote_address(request) or "unknown"


# Create the limiter with in-memory storage
# Using in-memory is fine for single-server deployments
# For multi-server, would need Redis backend
limiter = Limiter(
    key_func=get_ip_only,
    default_limits=["100/minute"],  # Default fallback
    storage_uri="memory://",
)


# Rate limit constants
# These are intentionally generous - only meant to stop automated attacks,
# never legitimate users (even many users behind shared NAT)

# Registration: Prevent mass UUID generation attacks
RATE_LIMIT_REGISTRATION = "100/hour"

# Client verification: Prevent UUID enumeration attacks
RATE_LIMIT_VERIFY = "120/minute"

# WebRTC offer: Allow frequent reconnections
RATE_LIMIT_OFFER = "120/minute"

# ICE candidate patches: WebRTC can be very chatty during setup
RATE_LIMIT_ICE = "500/minute"

# Static config endpoints: Allow frequent polling
RATE_LIMIT_CONFIG = "200/minute"

# Runtime config endpoints (prompts, stt-timeout): Allow frequent updates
RATE_LIMIT_RUNTIME_CONFIG = "200/minute"

# Providers endpoint: Allow frequent reads
RATE_LIMIT_PROVIDERS = "200/minute"
