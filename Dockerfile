# Build stage
FROM rust:1.75-slim as builder

WORKDIR /usr/src/app
COPY . .

# Install SSL dependencies for the build and enable nightly
RUN apt-get update && \
    apt-get install -y pkg-config libssl-dev && \
    rm -rf /var/lib/apt/lists/* && \
    rustup default nightly

# Build the application with release optimizations
RUN cargo build --release

# Runtime stage
FROM ubuntu:22.04

WORKDIR /usr/local/bin

# Install the correct version of OpenSSL and certificates
RUN apt-get update && \
    apt-get install -y openssl libssl3 ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Copy the built binary from the builder stage
COPY --from=builder /usr/src/app/target/release/sonic-bot-rust .

# Run the binary
CMD ["sonic-bot-rust"]