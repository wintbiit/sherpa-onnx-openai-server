use anyhow::Context;
use sherpa_onnx_openai_server::{
    app::{build_router, state_from_parts},
    config::AppConfig,
    tts::{MockSynthesizer, SherpaSynthesizer, SpeechSynthesizer},
};
use std::{net::SocketAddr, sync::Arc};
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = AppConfig::from_env().context("failed to load configuration")?;
    let addr: SocketAddr = config
        .bind_addr
        .parse()
        .context("BIND_ADDR must be host:port")?;
    let synthesizer: Arc<dyn SpeechSynthesizer> =
        if std::env::var("MOCK_TTS").as_deref() == Ok("true") {
            Arc::new(MockSynthesizer)
        } else {
            Arc::new(
                SherpaSynthesizer::new(&config.tts, config.max_concurrent_synthesis)
                    .context("failed to initialize Kokoro TTS model")?,
            )
        };
    let app = build_router(state_from_parts(
        synthesizer,
        config.model_aliases,
        config.voice_map,
        config.api_key,
    ));

    let listener = TcpListener::bind(addr)
        .await
        .context("failed to bind listener")?;
    info!(%addr, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("server failed")
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
