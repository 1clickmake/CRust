# ---- Build Stage ----
FROM rust:1-bookworm AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# ---- Runtime Stage ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/rust_board /app/rust_board
COPY templates ./templates
COPY static ./static
COPY locales ./locales
COPY config.example.toml ./config.example.toml

RUN mkdir -p /app/uploads /app/data \
    && cp /app/config.example.toml /app/config.toml

EXPOSE 3000

CMD ["/app/rust_board"]
