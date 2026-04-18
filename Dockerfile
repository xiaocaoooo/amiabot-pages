# syntax=docker/dockerfile:1.7

FROM golang:1.25-alpine AS builder
WORKDIR /app

COPY go.mod go.sum ./
RUN unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy \
    && go mod download

COPY . .
RUN unset HTTP_PROXY HTTPS_PROXY ALL_PROXY http_proxy https_proxy all_proxy \
    && CGO_ENABLED=0 GOOS=linux GOARCH=amd64 go build -o /out/amiabot-pages .

FROM alpine:3.20
WORKDIR /app

RUN apk add --no-cache ca-certificates wget && adduser -D -u 10001 appuser

COPY --from=builder /out/amiabot-pages /app/amiabot-pages

COPY --from=builder /app/templates /app/templates

RUN mkdir -p /app/cache && chown appuser:appuser /app/cache

ENV PORT=8080
EXPOSE 8080

USER appuser

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD wget -qO- "http://127.0.0.1:${PORT}/health" >/dev/null || exit 1

ENTRYPOINT ["/app/amiabot-pages"]
