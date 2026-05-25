# Multi-stage Dockerfile for runtime-service
FROM rust:1.72 as builder
WORKDIR /usr/src/runtime-service
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates docker.io && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/runtime-service/target/release/runtime-service /usr/local/bin/runtime-service
ENV RUST_LOG=info
ENV EXECUTOR_IMAGE=python:3.12-slim
ENV DOCKER_HOST=unix:///var/run/docker.sock
EXPOSE 8081
CMD ["/usr/local/bin/runtime-service"]
