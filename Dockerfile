# mik - WASM HTTP Runtime
#
# Build:  docker build -t mik .
# Run:    docker run -v $(pwd):/app -p 3000:3000 mik
# Size:   ~49MB
#
# Features enabled:  rustls (TLS), otlp (tracing)
# Features disabled: registry (use volume mounts for modules)

# =============================================================================
# Stage 1: Build static binary with musl
# =============================================================================
FROM clux/muslrust:stable AS builder

WORKDIR /app

# Copy manifests for dependency caching
COPY Cargo.toml Cargo.lock ./

# Dummy build for dependency cache
RUN mkdir -p src benches \
    && echo "fn main() {}" > src/main.rs \
    && echo "fn main() {}" > benches/runtime_bench.rs \
    && echo "fn main() {}" > benches/runtime_benchmarks.rs \
    && cargo build --release --no-default-features --features "rustls,otlp" 2>/dev/null || true

# Copy source and build
COPY src ./src
COPY benches ./benches
RUN touch src/main.rs \
    && cargo build --release --no-default-features --features "rustls,otlp" \
    && strip /app/target/x86_64-unknown-linux-musl/release/mik

# =============================================================================
# Stage 2: Minimal runtime
# =============================================================================
FROM scratch

# TLS certificates
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Binary
COPY --from=builder /app/target/x86_64-unknown-linux-musl/release/mik /mik

WORKDIR /app

ENV PORT=3000 \
    RUST_LOG=info \
    SSL_CERT_FILE=/etc/ssl/certs/ca-certificates.crt

EXPOSE 3000

ENTRYPOINT ["/mik"]
CMD ["run"]
