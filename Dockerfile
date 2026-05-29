# syntax=docker/dockerfile:1

FROM rust:1.86-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM nvidia/cuda:12.6.3-cudnn-runtime-ubuntu22.04
RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends ca-certificates ffmpeg libgomp1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/sherpa-onnx-openai-server /usr/local/bin/sherpa-onnx-openai-server

ENV BIND_ADDR=0.0.0.0:8080 \
    SHERPA_ONNX_PROVIDER=cuda \
    SHERPA_ONNX_NUM_THREADS=1 \
    MAX_CONCURRENT_SYNTHESIS=1

EXPOSE 8080
ENTRYPOINT ["sherpa-onnx-openai-server"]
