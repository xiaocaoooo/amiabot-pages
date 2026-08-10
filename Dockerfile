# syntax=docker/dockerfile:1.7

# --- Chef base: toolchain + cargo-chef (pinned) ---
FROM rust:1.97.1-alpine AS chef

# musl/openssl for linking; curl/ca-certificates for utoipa-swagger-ui download
RUN apk add --no-cache \
    musl-dev \
    openssl-dev \
    openssl-libs-static \
    pkgconf \
    curl \
    ca-certificates

# Prefer static OpenSSL on musl
ENV OPENSSL_STATIC=1 \
    OPENSSL_NO_VENDOR=1

RUN cargo install cargo-chef --locked --version 0.1.77

WORKDIR /app

# --- Planner: compute dependency recipe ---
FROM chef AS planner

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo chef prepare --recipe-path recipe.json

# --- Builder: cook deps (cached) then build the app ---
FROM chef AS builder

COPY --from=planner /app/recipe.json recipe.json

# Build dependencies only; invalidated when Cargo.toml / Cargo.lock / dep graph changes
RUN cargo chef cook --release --recipe-path recipe.json

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked \
    && cp /app/target/release/amiabot-pages /app/amiabot-pages

# --- Runtime ---
FROM alpine:3.20

WORKDIR /app

# Install runtime dependencies and setup non-root user
RUN apk add --no-cache ca-certificates wget libgcc && adduser -D -u 10001 appuser

# Copy built binary from builder
COPY --from=builder /app/amiabot-pages /app/amiabot-pages

# Copy templates and static assets (runtime only; does not bust compile layers)
COPY templates /app/templates
COPY static /app/static

# Prepare cache directories and setup permissions
RUN mkdir -p /app/cache/images /app/cache/pjsk && chown -R appuser:appuser /app/cache

ENV PORT=8080
EXPOSE 8080

USER appuser

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD wget -qO- "http://127.0.0.1:${PORT}/health" >/dev/null || exit 1

ENTRYPOINT ["/app/amiabot-pages"]
