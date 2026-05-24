# Multi-stage Dockerfile for runtime-service
FROM rust:1.72 as builder
WORKDIR /usr/src/runtime-service
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates procps && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/runtime-service/target/release/runtime-service /usr/local/bin/runtime-service
ENV RUST_LOG=info
EXPOSE 8081
USER 1000:1000
CMD ["/usr/local/bin/runtime-service"]
