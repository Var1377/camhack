# Multi-stage build using Debian Trixie (newer glibc for Fargate)
FROM --platform=linux/amd64 rust:slim as builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build for release
RUN cargo build --release

# Final runtime image - same Debian version as builder (Trixie/Testing)
FROM --platform=linux/amd64 debian:trixie-slim

# Install minimal runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/master /usr/local/bin/master-node

# Expose HTTP port
EXPOSE 8080/tcp

# Run the master node with debug logging
ENV RUST_LOG=info
CMD ["master-node"]
