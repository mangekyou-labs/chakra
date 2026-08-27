# ─── Stage 1: Build Rust binaries ─────────────────────────────────────────────
FROM rust:1.87-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates crates/

# Build only the two Chakra binaries in release.
# Use a dummy main.rs trick for dependency caching in CI.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --bin chakra-api-server --bin chakra-market-data-worker && \
    cp target/release/chakra-api-server /usr/local/bin/chakra-api-server && \
    cp target/release/chakra-market-data-worker /usr/local/bin/chakra-market-data-worker

# ─── Stage 2: Minimal runtime ─────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates tini && \
    rm -rf /var/lib/apt/lists/*

RUN groupadd -r chakra && useradd -r -g chakra -d /app chakra

WORKDIR /app

# Binaries from builder
COPY --from=builder /usr/local/bin/chakra-api-server /usr/local/bin/chakra-api-server
COPY --from=builder /usr/local/bin/chakra-market-data-worker /usr/local/bin/chakra-market-data-worker

# Entrypoint script
COPY docker-entrypoint.sh /app/docker-entrypoint.sh
RUN chmod +x /app/docker-entrypoint.sh

# Non-root
RUN chown -R chakra:chakra /app
USER chakra

# tini is PID 1; entrypoint runs worker + api as children.
ENTRYPOINT ["tini", "--"]
CMD ["/app/docker-entrypoint.sh"]

# Health: API liveness
HEALTHCHECK --interval=15s --timeout=3s --start-period=30s --retries=3 \
    CMD curl -sf http://127.0.0.1:${PORT:-8080}/api/v1/health || exit 1
