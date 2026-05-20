# 多阶段构建
FROM rust:1.86-slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY . .
RUN cargo build --release

# 运行阶段
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -r -s /bin/false md-user

COPY --from=builder /app/target/release/md-server /usr/local/bin/

RUN mkdir -p /etc/md-server /var/log/md-server \
    && chown -R md-user:md-user /var/log/md-server

COPY config.yaml /etc/md-server/

USER md-user

HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8081/health || exit 1

EXPOSE 8081 50051

CMD ["md-server", "--config", "/etc/md-server/config.yaml"]
