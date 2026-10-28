FROM rust:bookworm AS builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./

RUN mkdir src && \
    echo "pub fn version() -> &'static str { \"2.0.0\" }" > src/lib.rs && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

COPY src ./src

RUN touch src/lib.rs src/main.rs && \
    cargo build --release --bin smartpot-datagenerator

FROM debian:bookworm-slim

LABEL org.opencontainers.image.title="SmartPot DataGenerator (Rust)" \
      org.opencontainers.image.description="Simulador de cultivos virtuales SmartPot en Rust: alto rendimiento, seguridad de memoria y contrato MQTT v1" \
      org.opencontainers.image.source="https://github.com/SmartPotTech/SmartPot-DataGenerator-Rust" \
      org.opencontainers.image.licenses="MIT"

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

RUN groupadd -g 1000 smartpot && \
    useradd -u 1000 -g smartpot -m -s /bin/sh smartpot

WORKDIR /app

COPY --from=builder /app/target/release/smartpot-datagenerator /app/smartpot-datagenerator

RUN chown -R smartpot:smartpot /app

USER 1000:1000

ENV SIMULATOR_PORT=8081 \
    RUST_LOG=info

EXPOSE 8081

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/app/smartpot-datagenerator", "--healthcheck"]

ENTRYPOINT ["/app/smartpot-datagenerator"]
