use anyhow::{bail, Context, Result};
use std::{collections::HashMap, env, path::Path};

const DEFAULT_MODELS: [&str; 3] = ["tts-1", "tts-1-hd", "gpt-4o-mini-tts"];

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub bind_addr: String,
    pub api_key: Option<String>,
    pub model_aliases: Vec<String>,
    pub voice_map: HashMap<String, i32>,
    pub max_concurrent_synthesis: usize,
    pub tts: KokoroConfig,
}

#[derive(Debug, Clone)]
pub struct KokoroConfig {
    pub model: String,
    pub voices: String,
    pub tokens: String,
    pub data_dir: String,
    pub lexicon: Option<String>,
    pub dict_dir: Option<String>,
    pub lang: Option<String>,
    pub provider: String,
    pub num_threads: i32,
    pub debug: bool,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let mock_tts = env::var("MOCK_TTS").as_deref() == Ok("true");
        let bind_addr = env_or("BIND_ADDR", "0.0.0.0:8080");
        let api_key = optional_env("OPENAI_API_KEY");
        let model_aliases = parse_csv_env("MODEL_ALIASES").unwrap_or_else(|| {
            DEFAULT_MODELS
                .iter()
                .map(|model| (*model).to_owned())
                .collect()
        });
        if model_aliases.is_empty() {
            bail!("MODEL_ALIASES must contain at least one model alias");
        }

        let voice_map = load_voice_map()?;
        let max_concurrent_synthesis = env_or("MAX_CONCURRENT_SYNTHESIS", "1")
            .parse::<usize>()
            .context("MAX_CONCURRENT_SYNTHESIS must be a positive integer")?;
        if max_concurrent_synthesis == 0 {
            bail!("MAX_CONCURRENT_SYNTHESIS must be greater than zero");
        }

        let tts = KokoroConfig {
            model: required_path("KOKORO_MODEL", mock_tts, ensure_file)?,
            voices: required_path("KOKORO_VOICES", mock_tts, ensure_file)?,
            tokens: required_path("KOKORO_TOKENS", mock_tts, ensure_file)?,
            data_dir: required_path("KOKORO_DATA_DIR", mock_tts, ensure_dir)?,
            lexicon: optional_existing_file("KOKORO_LEXICON")?,
            dict_dir: optional_existing_dir("KOKORO_DICT_DIR")?,
            lang: optional_env("KOKORO_LANG"),
            provider: env_or("SHERPA_ONNX_PROVIDER", "cuda"),
            num_threads: env_or("SHERPA_ONNX_NUM_THREADS", "1")
                .parse::<i32>()
                .context("SHERPA_ONNX_NUM_THREADS must be an integer")?,
            debug: env_or("SHERPA_ONNX_DEBUG", "false")
                .parse::<bool>()
                .context("SHERPA_ONNX_DEBUG must be true or false")?,
        };

        Ok(Self {
            bind_addr,
            api_key,
            model_aliases,
            voice_map,
            max_concurrent_synthesis,
            tts,
        })
    }
}

fn load_voice_map() -> Result<HashMap<String, i32>> {
    let mut map = default_voice_map();
    if let Some(raw) = optional_env("VOICE_MAP_JSON") {
        let configured: HashMap<String, i32> = serde_json::from_str(&raw)
            .context("VOICE_MAP_JSON must be a JSON object of voice names to speaker ids")?;
        map.extend(
            configured
                .into_iter()
                .map(|(voice, id)| (voice.to_lowercase(), id)),
        );
    }
    Ok(map)
}

pub fn default_voice_map() -> HashMap<String, i32> {
    [
        ("alloy", 0),
        ("echo", 1),
        ("fable", 2),
        ("onyx", 3),
        ("nova", 4),
        ("shimmer", 5),
    ]
    .into_iter()
    .map(|(voice, id)| (voice.to_owned(), id))
    .collect()
}

fn parse_csv_env(key: &str) -> Option<Vec<String>> {
    optional_env(key).map(|value| {
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(str::to_owned)
            .collect()
    })
}

fn required_path(
    key: &str,
    skip_exists_check: bool,
    ensure: fn(&str, &str) -> Result<()>,
) -> Result<String> {
    let value = env::var(key).with_context(|| format!("{key} is required"))?;
    if !skip_exists_check {
        ensure(key, &value)?;
    }
    Ok(value)
}

fn optional_existing_file(key: &str) -> Result<Option<String>> {
    optional_env(key)
        .map(|value| {
            ensure_file(key, &value)?;
            Ok(value)
        })
        .transpose()
}

fn optional_existing_dir(key: &str) -> Result<Option<String>> {
    optional_env(key)
        .map(|value| {
            ensure_dir(key, &value)?;
            Ok(value)
        })
        .transpose()
}

fn ensure_file(key: &str, value: &str) -> Result<()> {
    let path = Path::new(value);
    if !path.is_file() {
        bail!("{key} does not point to an existing file: {value}");
    }
    Ok(())
}

fn ensure_dir(key: &str, value: &str) -> Result<()> {
    let path = Path::new(value);
    if !path.is_dir() {
        bail!("{key} does not point to an existing directory: {value}");
    }
    Ok(())
}

fn optional_env(key: &str) -> Option<String> {
    env::var(key).ok().filter(|value| !value.trim().is_empty())
}

fn env_or(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| default.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_voice_map_contains_openai_voice_names() {
        let map = default_voice_map();
        assert_eq!(map.get("alloy"), Some(&0));
        assert_eq!(map.get("shimmer"), Some(&5));
    }
}
