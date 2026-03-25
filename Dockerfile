# ── Build stage ─────────────────────────────────────────────
FROM python:3.12-slim AS builder

WORKDIR /app
COPY requirements.txt .
RUN pip install --no-cache-dir --target=/app/deps -r requirements.txt

# ── Runtime stage ───────────────────────────────────────────
FROM python:3.12-slim

LABEL maintainer="your-team@company.com"
LABEL description="Confluence Server MCP Server for Claude AI"

# Security: run as non-root user
RUN groupadd -r mcpuser && useradd -r -g mcpuser -d /app mcpuser

WORKDIR /app

# Copy dependencies and source
COPY --from=builder /app/deps /app/deps
COPY config.py confluence_client.py server.py ./

# Add deps to Python path
ENV PYTHONPATH=/app/deps:$PYTHONPATH
ENV PYTHONUNBUFFERED=1

# Default to HTTP transport for remote deployment
ENV MCP_TRANSPORT=http
ENV MCP_HOST=0.0.0.0
ENV MCP_PORT=8000

EXPOSE 8000

USER mcpuser

# Health check — the MCP HTTP server responds on /
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8000/mcp')" || exit 1

ENTRYPOINT ["python", "server.py"]
