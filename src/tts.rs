use crate::{audio::SynthesizedAudio, config::KokoroConfig};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use sherpa_onnx::{
    GenerationConfig, OfflineTts, OfflineTtsConfig, OfflineTtsKokoroModelConfig,
    OfflineTtsModelConfig,
};
use std::sync::Arc;
use tokio::sync::Semaphore;

#[async_trait]
pub trait SpeechSynthesizer: Send + Sync {
    async fn synthesize(
        &self,
        input: String,
        speaker_id: i32,
        speed: f32,
    ) -> Result<SynthesizedAudio>;
}

pub struct SherpaSynthesizer {
    tts: Arc<OfflineTts>,
    semaphore: Arc<Semaphore>,
}

pub struct MockSynthesizer;

#[async_trait]
impl SpeechSynthesizer for MockSynthesizer {
    async fn synthesize(
        &self,
        _input: String,
        _speaker_id: i32,
        _speed: f32,
    ) -> Result<SynthesizedAudio> {
        Ok(SynthesizedAudio {
            samples: vec![0.0; 24_000 / 10],
            sample_rate: 24_000,
        })
    }
}

impl SherpaSynthesizer {
    pub fn new(config: &KokoroConfig, max_concurrent: usize) -> Result<Self> {
        let sherpa_config = OfflineTtsConfig {
            model: OfflineTtsModelConfig {
                kokoro: OfflineTtsKokoroModelConfig {
                    model: Some(config.model.clone()),
                    voices: Some(config.voices.clone()),
                    tokens: Some(config.tokens.clone()),
                    data_dir: Some(config.data_dir.clone()),
                    lexicon: config.lexicon.clone(),
                    dict_dir: config.dict_dir.clone(),
                    lang: config.lang.clone(),
                    ..Default::default()
                },
                provider: Some(config.provider.clone()),
                num_threads: config.num_threads,
                debug: config.debug,
                ..Default::default()
            },
            ..Default::default()
        };

        let tts = OfflineTts::create(&sherpa_config)
            .ok_or_else(|| anyhow!("failed to create sherpa-onnx OfflineTts"))?;
        Ok(Self {
            tts: Arc::new(tts),
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
        })
    }
}

#[async_trait]
impl SpeechSynthesizer for SherpaSynthesizer {
    async fn synthesize(
        &self,
        input: String,
        speaker_id: i32,
        speed: f32,
    ) -> Result<SynthesizedAudio> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .context("synthesis semaphore was closed")?;
        let tts = Arc::clone(&self.tts);

        tokio::task::spawn_blocking(move || {
            let generation = GenerationConfig {
                speed,
                sid: speaker_id,
                ..Default::default()
            };
            let generated = tts
                .generate_with_config(&input, &generation, Option::<fn(&[f32], f32) -> bool>::None)
                .ok_or_else(|| anyhow!("sherpa-onnx failed to synthesize audio"))?;
            Ok(SynthesizedAudio {
                samples: generated.samples().to_vec(),
                sample_rate: generated.sample_rate(),
            })
        })
        .await
        .context("synthesis worker panicked")?
    }
}
