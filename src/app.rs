use crate::{
    audio::{encode_audio, format_name, SpeechResponseFormatExt},
    error::ApiError,
    tts::SpeechSynthesizer,
};
use async_openai::types::audio::{CreateSpeechRequest, SpeechModel, SpeechResponseFormat, Voice};
use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

const MAX_INPUT_CHARS: usize = 4096;

#[derive(Clone)]
pub struct AppState {
    pub synthesizer: Arc<dyn SpeechSynthesizer>,
    pub model_aliases: Arc<HashSet<String>>,
    pub model_list: Arc<Vec<String>>,
    pub voice_map: Arc<HashMap<String, i32>>,
    pub api_key: Option<Arc<String>>,
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/models", get(list_models))
        .route("/v1/audio/speech", post(create_speech))
        .with_state(state)
}

pub fn state_from_parts(
    synthesizer: Arc<dyn SpeechSynthesizer>,
    model_aliases: Vec<String>,
    voice_map: HashMap<String, i32>,
    api_key: Option<String>,
) -> AppState {
    AppState {
        synthesizer,
        model_aliases: Arc::new(model_aliases.iter().cloned().collect()),
        model_list: Arc::new(model_aliases),
        voice_map: Arc::new(
            voice_map
                .into_iter()
                .map(|(k, v)| (k.to_lowercase(), v))
                .collect(),
        ),
        api_key: api_key.map(Arc::new),
    }
}

async fn healthz() -> &'static str {
    "ok"
}

async fn list_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<ModelList>, ApiError> {
    authorize(&state, &headers)?;
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let data = state
        .model_list
        .iter()
        .map(|id| Model {
            id: id.clone(),
            object: "model",
            created,
            owned_by: "local",
        })
        .collect();
    Ok(Json(ModelList {
        object: "list",
        data,
    }))
}

async fn create_speech(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<CreateSpeechRequest>,
) -> Result<Response, ApiError> {
    authorize(&state, &headers)?;
    let normalized = validate_request(request, &state)?;
    let audio = state
        .synthesizer
        .synthesize(normalized.input, normalized.speaker_id, normalized.speed)
        .await
        .map_err(|err| ApiError::internal(err.to_string()))?;
    let bytes = encode_audio(&audio, normalized.response_format)
        .map_err(|err| ApiError::internal(err.to_string()))?;

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            normalized.response_format.content_type(),
        )
        .body(Body::from(bytes))
        .map_err(|err| ApiError::internal(err.to_string()))
}

fn authorize(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(expected) = &state.api_key else {
        return Ok(());
    };

    let Some(actual) = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
    else {
        return Err(ApiError::unauthorized("Missing Authorization header"));
    };
    let Some(token) = actual.strip_prefix("Bearer ") else {
        return Err(ApiError::unauthorized("Invalid Authorization header"));
    };
    if token != expected.as_str() {
        return Err(ApiError::unauthorized("Invalid API key"));
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct NormalizedSpeechRequest {
    input: String,
    speaker_id: i32,
    response_format: SpeechResponseFormat,
    speed: f32,
}

fn validate_request(
    request: CreateSpeechRequest,
    state: &AppState,
) -> Result<NormalizedSpeechRequest, ApiError> {
    let model = speech_model_id(&request.model);
    if !state.model_aliases.contains(&model) {
        return Err(ApiError::invalid_request(
            format!("Unknown model: {model}"),
            "model",
        ));
    }

    let input = request.input;
    if input.is_empty() {
        return Err(ApiError::invalid_request("input is required", "input"));
    }
    if input.chars().count() > MAX_INPUT_CHARS {
        return Err(ApiError::invalid_request(
            "input must be 4096 characters or fewer",
            "input",
        ));
    }

    let voice = voice_id(&request.voice)?;
    let Some(&speaker_id) = state.voice_map.get(&voice) else {
        return Err(ApiError::invalid_request(
            format!("Unknown voice: {voice}"),
            "voice",
        ));
    };

    let speed = request.speed.unwrap_or(1.0);
    if !(0.25..=4.0).contains(&speed) {
        return Err(ApiError::invalid_request(
            "speed must be between 0.25 and 4.0",
            "speed",
        ));
    }

    let response_format = request.response_format.unwrap_or(SpeechResponseFormat::Mp3);
    if !response_format.is_supported_by_service() {
        return Err(ApiError::invalid_request(
            format!(
                "Unsupported response_format: {}",
                format_name(response_format)
            ),
            "response_format",
        ));
    }

    Ok(NormalizedSpeechRequest {
        input,
        speaker_id,
        response_format,
        speed,
    })
}

fn speech_model_id(model: &SpeechModel) -> String {
    match model {
        SpeechModel::Tts1 => "tts-1".to_owned(),
        SpeechModel::Tts1Hd => "tts-1-hd".to_owned(),
        SpeechModel::Gpt4oMiniTts => "gpt-4o-mini-tts".to_owned(),
        SpeechModel::Other(model) => model.clone(),
    }
}

fn voice_id(voice: &Voice) -> Result<String, ApiError> {
    let value = serde_json::to_value(voice).map_err(|err| ApiError::internal(err.to_string()))?;
    value
        .as_str()
        .map(|voice| voice.to_lowercase())
        .ok_or_else(|| ApiError::invalid_request("Custom voice objects are not supported", "voice"))
}

#[derive(Debug, Serialize)]
struct ModelList {
    object: &'static str,
    data: Vec<Model>,
}

#[derive(Debug, Serialize)]
struct Model {
    id: String,
    object: &'static str,
    created: u64,
    owned_by: &'static str,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{audio::SynthesizedAudio, config::default_voice_map};
    use anyhow::Result;
    use async_trait::async_trait;
    use axum::body::to_bytes;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    struct FakeSynthesizer;

    #[async_trait]
    impl SpeechSynthesizer for FakeSynthesizer {
        async fn synthesize(
            &self,
            _input: String,
            _speaker_id: i32,
            _speed: f32,
        ) -> Result<SynthesizedAudio> {
            Ok(SynthesizedAudio {
                samples: vec![0.0, 0.25, -0.25],
                sample_rate: 24_000,
            })
        }
    }

    fn test_app(api_key: Option<&str>) -> Router {
        let state = state_from_parts(
            Arc::new(FakeSynthesizer),
            vec!["tts-1".to_owned(), "gpt-4o-mini-tts".to_owned()],
            default_voice_map(),
            api_key.map(str::to_owned),
        );
        build_router(state)
    }

    async fn post_speech(body: serde_json::Value) -> Response {
        test_app(None)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/audio/speech")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn healthz_returns_ok() {
        let response = test_app(None)
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn models_requires_auth_when_key_is_configured() {
        let response = test_app(Some("secret"))
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        let response = test_app(Some("secret"))
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .header(header::AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn models_returns_aliases() {
        let response = test_app(None)
            .oneshot(
                Request::builder()
                    .uri("/v1/models")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["object"], "list");
        assert_eq!(json["data"][0]["id"], "tts-1");
    }

    #[tokio::test]
    async fn validates_missing_input() {
        let response = post_speech(serde_json::json!({"model":"tts-1","voice":"alloy"})).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn validates_too_long_input() {
        let response = post_speech(serde_json::json!({
            "model": "tts-1",
            "input": "a".repeat(4097),
            "voice": "alloy"
        }))
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn validates_speed() {
        let response = post_speech(serde_json::json!({
            "model": "tts-1",
            "input": "hello",
            "voice": "alloy",
            "speed": 4.01
        }))
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn validates_unknown_voice() {
        let response = post_speech(serde_json::json!({
            "model": "tts-1",
            "input": "hello",
            "voice": "missing"
        }))
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejects_unknown_format_at_json_layer() {
        let response = post_speech(serde_json::json!({
            "model": "tts-1",
            "input": "hello",
            "voice": "alloy",
            "response_format": "unknown"
        }))
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn rejects_format_not_supported_by_service() {
        let response = post_speech(serde_json::json!({
            "model": "tts-1",
            "input": "hello",
            "voice": "alloy",
            "response_format": "opus"
        }))
        .await;
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn returns_requested_wav_content_type() {
        let response = post_speech(serde_json::json!({
            "model": "tts-1",
            "input": "hello",
            "voice": "alloy",
            "response_format": "wav"
        }))
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "audio/wav");
    }

    #[tokio::test]
    async fn error_shape_matches_openai_style() {
        let response =
            post_speech(serde_json::json!({"model":"missing","input":"hello","voice":"alloy"}))
                .await;
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["error"]["type"], "invalid_request_error");
        assert_eq!(json["error"]["param"], "model");
    }
}
