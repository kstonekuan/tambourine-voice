FROM python:3.13-slim

# Install system dependencies for OpenCV and audio processing
RUN apt-get update && apt-get install -y --no-install-recommends \
    libgl1 \
    libglib2.0-0 \
    ffmpeg \
    && rm -rf /var/lib/apt/lists/*

# Install uv for fast dependency management
COPY --from=ghcr.io/astral-sh/uv:latest /uv /usr/local/bin/uv

WORKDIR /app

# Copy README first (referenced by pyproject.toml)
# Build with: docker build -f server/Dockerfile .  (from repo root)
COPY README.md ../README.md

# Copy dependency files (for layer caching)
COPY server/pyproject.toml server/uv.lock ./

# Install dependencies only (skip installing the workspace package)
RUN uv sync --frozen --no-dev --no-install-workspace

# Copy server source code
COPY server/ .

# Install the workspace package now that source is available
RUN uv sync --frozen --no-dev

# Expose the server port
EXPOSE 8765

# Run the server
CMD ["uv", "run", "python", "main.py", "--host", "0.0.0.0"]
