# mik - WASM HTTP host runtime (Minimal variant - RECOMMENDED)
# Optimized multi-stage build for minimal scratch image
#
# Build: docker build -t mik .
# Size: ~47MB (smallest possible)
#
# Benchmark Results (10k requests, 50 concurrent):
#   - ~75k req/sec (p50: 0.3ms, p95: 1.7ms, p99: 2.8ms)
#   - Comparable or better than glibc variant!
#
# Uses:
#   - clux/muslrust: Pre-configured musl build environment
#   - mimalloc: Fast allocator (fixes musl multi-core performance)
#   - scratch: Zero-overhead base image
#   - --no-default-features: Excludes git2/oci-client for smaller binary
#
# For glibc-based builds (~85MB):
#   docker build -f Dockerfile.distroless -t mik:distroless .

# =============================================================================
# Stage 1: Build with musl for fully static binary
# =============================================================================
FROM clux/muslrust:stable AS builder

WORKDIR /app

# Copy manifests first for layer caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src for dependency caching
RUN mkdir -p src && echo "fn main() {}" > src/main.rs

# Build dependencies only (cached layer) - without registry features
RUN cargo build --release --no-default-features 2>/dev/null || true

# Copy actual source
COPY src ./src

# Touch main.rs to invalidate the dummy
RUN touch src/main.rs

# Build the final binary (no registry features = smaller binary)
RUN cargo build --release --no-default-features

# Strip the binary for smaller size
RUN strip /app/target/x86_64-unknown-linux-musl/release/mik

# =============================================================================
# Stage 2: Minimal runtime (scratch)
# =============================================================================
FROM scratch

# Copy CA certificates for HTTPS requests (from builder)
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Copy the static binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/mik /mik

# Create app directory structure
WORKDIR /app

# Default environment
ENV PORT=3000
ENV RUST_LOG=info
ENV SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

EXPOSE 3000

# Run mik (no shell available in scratch)
ENTRYPOINT ["/mik"]
CMD ["run"]
