# syntax=docker/dockerfile:1

ARG RUNTIME_IMAGE=nvidia/cuda:12.6.3-cudnn-runtime-ubuntu22.04
ARG DEFAULT_PROVIDER=cuda

FROM rust:1.86-bookworm AS builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM ${RUNTIME_IMAGE}
ARG DEFAULT_PROVIDER=cuda
RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends ca-certificates curl ffmpeg libgomp1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/sherpa-onnx-openai-server /usr/local/bin/sherpa-onnx-openai-server

ENV BIND_ADDR=0.0.0.0:8080 \
    SHERPA_ONNX_PROVIDER=${DEFAULT_PROVIDER} \
    SHERPA_ONNX_NUM_THREADS=1 \
    MAX_CONCURRENT_SYNTHESIS=1

EXPOSE 8080
ENTRYPOINT ["sherpa-onnx-openai-server"]
