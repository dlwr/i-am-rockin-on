# Build stage
FROM rust:1.95-slim AS builder
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev curl \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown && \
    cargo install cargo-leptos --locked --version 0.3.6

WORKDIR /app
COPY . .
ENV SQLX_OFFLINE=true
RUN cargo leptos build --release

# Runtime stage
FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 sqlite3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/i-am-rockin-on /app/
COPY --from=builder /app/target/release/scrape /app/
COPY --from=builder /app/target/site /app/site
COPY migrations /app/migrations

ENV LEPTOS_OUTPUT_NAME=i-am-rockin-on
ENV LEPTOS_SITE_ROOT=site
ENV LEPTOS_SITE_PKG_DIR=pkg
ENV LEPTOS_SITE_ADDR=0.0.0.0:3000
ENV LEPTOS_ENV=PROD
EXPOSE 3000
CMD ["/app/i-am-rockin-on"]
