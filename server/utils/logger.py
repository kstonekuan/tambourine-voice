"""Logging configuration using loguru for voice agent platform."""

import os
import sys
from typing import TYPE_CHECKING

from loguru import logger

if TYPE_CHECKING:
    from loguru import Record


def _should_log(record: "Record") -> bool:
    """Filter out known harmless warnings from pipecat transport layer."""
    module_name = record["name"]
    message = record["message"]

    is_smallwebrtc_timeout = (
        module_name == "pipecat.transports.smallwebrtc.transport"
        and "Timeout: No audio frame received" in message
    )
    is_base_input_timeout = (
        module_name == "pipecat.transports.base_input"
        and "audio not received for more than" in message
    )

    return not (is_smallwebrtc_timeout or is_base_input_timeout)


def _log_format(record: "Record") -> str:
    """Custom format function that only shows extra when present."""
    base = (
        "<green>{time:YYYY-MM-DD HH:mm:ss.SSS}</green> | "
        "<level>{level: <8}</level> | "
        "<cyan>{name}</cyan>:<cyan>{function}</cyan>:<cyan>{line}</cyan> - "
        "<level>{message}</level>"
    )
    if record["extra"]:
        return base + " {extra}\n{exception}"
    return base + "\n{exception}"


def configure_logging(log_level: str | None = None) -> None:
    """
    Configure loguru logging with sensible defaults.

    Args:
        log_level: Optional log level to use. If provided, takes precedence over
                   the LOG_LEVEL environment variable. Defaults to "INFO" if neither
                   is set.

    Configures log level and sets up colored output to stdout.
    """
    if log_level is not None:
        log_level_str = log_level.upper()
    else:
        log_level_str = os.getenv("LOG_LEVEL", "INFO").upper()

    # Remove default handler
    logger.remove()

    # Add custom handler with colorization and formatting
    logger.add(
        sys.stdout,
        format=_log_format,
        level=log_level_str,
        colorize=True,
        filter=_should_log,
    )
