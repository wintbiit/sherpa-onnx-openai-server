# sherpa-onnx-openai-server

Small Rust HTTP service that exposes a core OpenAI-compatible text-to-speech endpoint backed by `sherpa-onnx` Kokoro `OfflineTts`.

## API

- `GET /healthz`
- `GET /v1/models`
- `POST /v1/audio/speech`

`POST /v1/audio/speech` uses `async-openai` request types for the OpenAI speech schema. This service supports string voices mapped to Kokoro speaker ids and these output formats:

- `mp3` default, encoded with `ffmpeg`
- `wav`
- `pcm` little-endian signed 16-bit mono PCM
- `flac`, encoded with `ffmpeg`

`opus`, `aac`, custom voice objects, streaming, custom voice creation, and `/v1/audio/voices` are not implemented.

## Recommended Model

For this service, use `kokoro-multi-lang-v1_1` from the official k2-fsa `sherpa-onnx` TTS model release. It is the newer Kokoro Chinese + English package, has 103 speakers, and is listed by the sherpa-onnx Kokoro documentation as both normal precision and int8 variants:

- `kokoro-multi-lang-v1_1.tar.bz2`
- `kokoro-int8-multi-lang-v1_1.tar.bz2`

Use the normal precision package for the `:cuda` image unless image/disk size is the priority. Use the int8 package for the `:cpu` image or smaller deployments.

Download:

```powershell
mkdir D:\models
curl.exe -L https://github.com/k2-fsa/sherpa-onnx/releases/download/tts-models/kokoro-multi-lang-v1_1.tar.bz2 -o D:\models\kokoro-multi-lang-v1_1.tar.bz2
tar -xf D:\models\kokoro-multi-lang-v1_1.tar.bz2 -C D:\models
```

The older `kokoro-multi-lang-v1_0` is also supported and documented by k2-fsa. It has fewer speakers, but the same simple layout: `model.onnx`, `voices.bin`, `tokens.txt`, and `espeak-ng-data`.

The local and GitHub Actions smoke tests use `MOCK_TTS=true` because CI does not mount a real model. That verifies the HTTP API, GHCR image startup, and audio encoding path; it does not prove a real Kokoro model can synthesize on your GPU. Use the Docker run command below for real-model verification.

References:

- https://k2-fsa.github.io/sherpa/onnx/tts/pretrained_models/kokoro.html
- https://github.com/k2-fsa/sherpa-onnx/releases/tag/tts-models

## Configuration

Recommended:

- `MODEL_DIR`, the mounted model root
- `MODEL_NAME`, optional subdirectory under `MODEL_DIR`

With `MODEL_DIR=/models` and `MODEL_NAME=kokoro`, the service looks for:

- `/models/kokoro/model.onnx`
- `/models/kokoro/voices.bin`
- `/models/kokoro/tokens.txt`
- `/models/kokoro/espeak-ng-data`
- `/models/kokoro/lexicon-us-en.txt`, if present
- `/models/kokoro/lexicon-zh.txt`, if present
- `/models/kokoro/dict`, if present

Advanced overrides:

- `KOKORO_MODEL`
- `KOKORO_VOICES`
- `KOKORO_TOKENS`
- `KOKORO_DATA_DIR`

Optional:

- `KOKORO_MODEL_FILE`, default `model.onnx`
- `KOKORO_VOICES_FILE`, default `voices.bin`
- `KOKORO_TOKENS_FILE`, default `tokens.txt`
- `KOKORO_DATA_DIR_NAME`, default `espeak-ng-data`, with `data/espeak-ng-data` also detected
- `KOKORO_LEXICON`, comma-separated paths; inferred from `lexicon-us-en.txt,lexicon-zh.txt` if present
- `KOKORO_DICT_DIR`, inferred from `dict` if present
- `KOKORO_LANG`
- `VOICE_MAP_JSON`, for example `{"alloy":0,"nova":4}`
- `MODEL_ALIASES`, comma-separated. Default: `tts-1,tts-1-hd,gpt-4o-mini-tts`
- `OPENAI_API_KEY`, enables Bearer auth when set
- `BIND_ADDR`, default `0.0.0.0:8080`
- `MAX_CONCURRENT_SYNTHESIS`, default `1`
- `SHERPA_ONNX_PROVIDER`, default `cuda`
- `SHERPA_ONNX_NUM_THREADS`, default `1`
- `SHERPA_ONNX_DEBUG`, default `false`

Default voice ids:

| Voice | Speaker id |
| --- | ---: |
| `alloy` | 0 |
| `echo` | 1 |
| `fable` | 2 |
| `onyx` | 3 |
| `nova` | 4 |
| `shimmer` | 5 |

## Docker

Build:

```powershell
docker build -t sherpa-onnx-openai-server .
```

Run with a mounted Kokoro model directory:

```powershell
docker run --rm --gpus all -p 8080:8080 `
  -v D:\models\kokoro-multi-lang-v1_1:/models/kokoro:ro `
  -e MODEL_DIR=/models `
  -e MODEL_NAME=kokoro `
  ghcr.io/wintbiit/sherpa-onnx-openai-server:cuda
```

CPU image:

```powershell
docker run --rm -p 8080:8080 `
  -v D:\models\kokoro-int8-multi-lang-v1_1:/models/kokoro:ro `
  -e MODEL_DIR=/models `
  -e MODEL_NAME=kokoro `
  ghcr.io/wintbiit/sherpa-onnx-openai-server:cpu
```

Smoke test:

```powershell
curl.exe http://localhost:8080/v1/audio/speech `
  -H "Content-Type: application/json" `
  -d "{\"model\":\"tts-1\",\"voice\":\"alloy\",\"input\":\"Hello, 你好。\",\"response_format\":\"wav\"}" `
  --output speech.wav
```

If `OPENAI_API_KEY` is set, include:

```text
Authorization: Bearer <token>
```

## Development

```powershell
cargo test
```
