# Build stage
FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev openssl-dev openssl-libs-static

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release -p relay-server && \
    strip target/release/cc-relay-server

# Runtime stage
FROM alpine:3.21

LABEL org.opencontainers.image.source="https://github.com/wakaka6/claude-code-relay"
LABEL org.opencontainers.image.description="High-performance AI API relay service"
LABEL org.opencontainers.image.licenses="MIT"

RUN apk add --no-cache ca-certificates tzdata curl && \
    adduser -D -h /app cc-relay

WORKDIR /app

COPY --from=builder /app/target/release/cc-relay-server /usr/local/bin/
COPY config.example.toml /app/config.example.toml

RUN mkdir -p /app/data && chown -R cc-relay:cc-relay /app

USER cc-relay

EXPOSE 3000

ENV RUST_LOG=info

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

ENTRYPOINT ["cc-relay-server"]
CMD ["--config", "/app/config.toml"]
