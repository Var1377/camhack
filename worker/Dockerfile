# Multi-stage build using Debian Trixie (newer glibc for Fargate)
FROM rust:slim as builder

WORKDIR /app

# Install protobuf compiler for tonic-build
RUN apt-get update && \
    apt-get install -y protobuf-compiler && \
    rm -rf /var/lib/apt/lists/*

# Copy manifests and build script
COPY Cargo.toml Cargo.lock build.rs ./

# Copy proto files for gRPC code generation
COPY proto ./proto

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
FROM debian:trixie-slim

# Install minimal runtime dependencies
RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy binary from builder
COPY --from=builder /app/target/release/worker /usr/local/bin/udp-node

# Expose ports
EXPOSE 5000/tcp
EXPOSE 8080/udp
EXPOSE 8081/tcp

# Run the UDP node
CMD ["udp-node"]
