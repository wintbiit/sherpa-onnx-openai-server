# syntax=docker/dockerfile:1

ARG RUNTIME_IMAGE=nvidia/cuda:12.6.3-cudnn-runtime-ubuntu22.04
ARG DEFAULT_PROVIDER=cuda

FROM rust:1.86-bookworm AS builder
WORKDIR /app

ARG SHERPA_ONNX_PREBUILT_URL=
RUN if [ -n "$SHERPA_ONNX_PREBUILT_URL" ]; then \
      apt-get -o Acquire::Retries=5 update \
      && apt-get -o Acquire::Retries=5 install -y --no-install-recommends ca-certificates curl bzip2 \
      && rm -rf /var/lib/apt/lists/* \
      && mkdir -p /opt/sherpa-onnx-prebuilt \
      && curl -fsSL "$SHERPA_ONNX_PREBUILT_URL" -o /tmp/sherpa-onnx-prebuilt.tar.bz2 \
      && tar -xjf /tmp/sherpa-onnx-prebuilt.tar.bz2 -C /opt/sherpa-onnx-prebuilt --strip-components=1; \
    fi

COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN if [ -n "$SHERPA_ONNX_PREBUILT_URL" ]; then \
      SHERPA_ONNX_LIB_DIR=/opt/sherpa-onnx-prebuilt/lib cargo build --release; \
    else \
      cargo build --release; \
    fi

FROM ${RUNTIME_IMAGE}
ARG DEFAULT_PROVIDER=cuda
RUN apt-get -o Acquire::Retries=5 update \
    && apt-get -o Acquire::Retries=5 install -y --no-install-recommends ca-certificates curl ffmpeg libgomp1 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY --from=builder /app/target/release/sherpa-onnx-openai-server /usr/local/bin/sherpa-onnx-openai-server
COPY --from=builder /app/target/release/*.so* /usr/local/lib/
RUN ldconfig

ENV BIND_ADDR=0.0.0.0:8080 \
    SHERPA_ONNX_PROVIDER=${DEFAULT_PROVIDER} \
    SHERPA_ONNX_NUM_THREADS=1 \
    MAX_CONCURRENT_SYNTHESIS=1

EXPOSE 8080
ENTRYPOINT ["sherpa-onnx-openai-server"]
