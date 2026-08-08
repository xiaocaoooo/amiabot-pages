# syntax=docker/dockerfile:1.7

# --- Build Stage ---
FROM rust:1.97.1-alpine AS builder

# Install build dependencies
RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

WORKDIR /app

# Cargo uses git or network sometimes, clear proxy in builder if required
RUN unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy

# Copy Cargo files for dependency caching
COPY Cargo.toml Cargo.lock ./

# Create dummy src/main.rs to build dependencies and cache them
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy actual source code and templates
COPY src ./src
COPY templates ./templates

# Build the actual application
# We need static linking for static openssl / musl, which is standard in alpine
RUN cargo build --release

# --- Runtime Stage ---
FROM alpine:3.20

WORKDIR /app

# Install runtime dependencies and setup non-root user
RUN apk add --no-cache ca-certificates wget libgcc && adduser -D -u 10001 appuser

# Copy built binary from builder
COPY --from=builder /app/target/release/amiabot-pages /app/amiabot-pages

# Copy templates and static assets
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
