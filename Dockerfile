# Multi-stage build for Neutrino
# Stage 1: Build Rust binary
FROM rust:1.83-slim as builder

WORKDIR /build

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build release binary
RUN cargo build --release

# Stage 2: Runtime image with Python
FROM python:3.11-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy Rust binary from builder
COPY --from=builder /build/target/release/neutrino-core /usr/local/bin/neutrino

# Copy Python code
COPY python/neutrino /app/python/neutrino
COPY python/cli /app/python/cli
COPY examples /app/examples

# Install Python dependencies
RUN pip install --no-cache-dir \
    msgpack>=1.0.0 \
    fastapi>=0.104.0 \
    uvicorn[standard]>=0.24.0 \
    pydantic>=2.0.0 \
    click>=8.0.0

# Set Python path to include our modules
ENV PYTHONPATH=/app/python:/app:$PYTHONPATH
ENV PYTHONUNBUFFERED=1

# Expose ports
# 8080: Main Neutrino HTTP server
# 8081: ASGI app (Uvicorn) in mounted mode
EXPOSE 8080 8081

# Default command: run neutrino
CMD ["neutrino"]
