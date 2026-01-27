# Build stage
FROM rustlang/rust:nightly-bookworm-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy the workspace files
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY schemas ./schemas
COPY examples ./examples
COPY hem-lambda ./hem-lambda
COPY hem-http ./hem-http

# Build the release binary
RUN cargo build --package hem-http --release

# Runtime stage
FROM debian:bookworm-slim

WORKDIR /app

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy the binary from builder
COPY --from=builder /app/target/release/hem-http /app/hem-http

# Set environment variables
ENV PORT=8080
ENV RUST_LOG=hem_http=info,tower_http=info

# Expose port
EXPOSE 8080

# Run the binary
CMD ["/app/hem-http"]
