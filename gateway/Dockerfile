FROM rustlang/rust:nightly-slim as builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy workspace files
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

# Build the gateway binary
RUN cargo build --release --bin neutrino-gateway

# Runtime stage - use same base as builder to match GLIBC
FROM debian:sid-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/neutrino-gateway /usr/local/bin/

# Create data directory
RUN mkdir -p /data

# Expose port
EXPOSE 8080

# Run the gateway
CMD ["neutrino-gateway"]
