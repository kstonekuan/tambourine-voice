"""FastAPI configuration router for Tambourine settings.

This module provides REST endpoints for:
- Getting default prompt sections

All runtime pipeline configuration (including provider info) is handled via
WebRTC data channel through ConfigurationHandler. This file only exposes
static configuration data that doesn't require pipeline access.
"""

from __future__ import annotations

from fastapi import APIRouter
from pydantic import BaseModel

from processors.llm import (
    ADVANCED_PROMPT_DEFAULT,
    DICTIONARY_PROMPT_DEFAULT,
    MAIN_PROMPT_DEFAULT,
)

config_router = APIRouter()


# =============================================================================
# Prompt Section Models and Endpoints
# =============================================================================


class DefaultSectionsResponse(BaseModel):
    """Response with default prompts for each section."""

    main: str
    advanced: str
    dictionary: str


@config_router.get("/api/prompt/sections/default", response_model=DefaultSectionsResponse)
async def get_default_sections() -> DefaultSectionsResponse:
    """Get default prompts for each section."""
    return DefaultSectionsResponse(
        main=MAIN_PROMPT_DEFAULT,
        advanced=ADVANCED_PROMPT_DEFAULT,
        dictionary=DICTIONARY_PROMPT_DEFAULT,
    )
