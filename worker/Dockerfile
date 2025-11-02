# Multi-stage build with static linking (musl)
FROM rust:alpine as builder

WORKDIR /app

# Install build dependencies (musl has protobuf compiler available)
RUN apk add --no-cache musl-dev protobuf-dev protoc

# Add musl target for static linking
RUN rustup target add x86_64-unknown-linux-musl

# Copy manifests and build script
COPY Cargo.toml Cargo.lock build.rs ./

# Copy proto files for gRPC code generation
COPY proto ./proto

# Create dummy main to cache dependencies
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release --target x86_64-unknown-linux-musl && \
    rm -rf src

# Copy actual source code
COPY src ./src

# Build for release with musl (statically linked, no glibc dependency)
RUN cargo build --release --target x86_64-unknown-linux-musl

# Final minimal image - can use alpine or even scratch since binary is static
FROM alpine:latest

# Install minimal runtime dependencies (just for CA certs)
RUN apk add --no-cache ca-certificates

# Copy statically linked binary from builder
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/worker /usr/local/bin/udp-node

# Expose ports
EXPOSE 5000/tcp
EXPOSE 8080/udp
EXPOSE 8081/tcp

# Run the UDP node
CMD ["udp-node"]
