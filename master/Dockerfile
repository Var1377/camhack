# Multi-stage build with static linking (musl) - eliminates glibc version issues
FROM --platform=linux/amd64 rust:alpine as builder

WORKDIR /app

# Install build dependencies
RUN apk add --no-cache musl-dev

# Add musl target for static linking
RUN rustup target add x86_64-unknown-linux-musl

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build for release with musl (statically linked, no glibc dependency)
RUN cargo build --release --target x86_64-unknown-linux-musl

# Final minimal image - can use alpine since binary is statically linked
FROM --platform=linux/amd64 alpine:latest

# Install minimal runtime dependencies
RUN apk add --no-cache ca-certificates

# Copy statically linked binary from builder
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/master /usr/local/bin/master-node

# Expose HTTP port
EXPOSE 8080/tcp

# Run the master node with debug logging
ENV RUST_LOG=info
CMD ["master-node"]
