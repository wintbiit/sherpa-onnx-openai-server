use anyhow::{anyhow, bail, Context, Result};
use async_openai::types::audio::SpeechResponseFormat;
use std::{
    io::Write,
    process::{Command, Stdio},
};

pub trait SpeechResponseFormatExt {
    fn content_type(self) -> &'static str;
    fn is_supported_by_service(self) -> bool;
}

impl SpeechResponseFormatExt for SpeechResponseFormat {
    fn content_type(self) -> &'static str {
        match self {
            Self::Wav => "audio/wav",
            Self::Pcm => "audio/pcm",
            Self::Mp3 => "audio/mpeg",
            Self::Flac => "audio/flac",
            Self::Opus => "audio/ogg",
            Self::Aac => "audio/aac",
        }
    }

    fn is_supported_by_service(self) -> bool {
        matches!(self, Self::Wav | Self::Pcm | Self::Mp3 | Self::Flac)
    }
}

pub fn format_name(format: SpeechResponseFormat) -> &'static str {
    match format {
        SpeechResponseFormat::Mp3 => "mp3",
        SpeechResponseFormat::Opus => "opus",
        SpeechResponseFormat::Aac => "aac",
        SpeechResponseFormat::Flac => "flac",
        SpeechResponseFormat::Pcm => "pcm",
        SpeechResponseFormat::Wav => "wav",
    }
}

pub fn encode_audio(audio: &SynthesizedAudio, format: SpeechResponseFormat) -> Result<Vec<u8>> {
    match format {
        SpeechResponseFormat::Wav => Ok(float_pcm_to_wav(&audio.samples, audio.sample_rate)?),
        SpeechResponseFormat::Pcm => Ok(float_pcm_to_s16le(&audio.samples)),
        SpeechResponseFormat::Mp3 => encode_with_ffmpeg(audio, "mp3"),
        SpeechResponseFormat::Flac => encode_with_ffmpeg(audio, "flac"),
        SpeechResponseFormat::Opus | SpeechResponseFormat::Aac => {
            bail!("unsupported response_format: {}", format_name(format))
        }
    }
}

#[derive(Debug, Clone)]
pub struct SynthesizedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: i32,
}

pub fn float_pcm_to_s16le(samples: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(samples.len() * 2);
    for sample in samples {
        let clamped = sample.clamp(-1.0, 1.0);
        let value = (clamped * i16::MAX as f32).round() as i16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

pub fn float_pcm_to_wav(samples: &[f32], sample_rate: i32) -> Result<Vec<u8>> {
    if sample_rate <= 0 {
        bail!("sample rate must be positive");
    }

    let pcm = float_pcm_to_s16le(samples);
    let data_len = pcm.len() as u32;
    let byte_rate = sample_rate as u32 * 2;
    let block_align = 2u16;
    let bits_per_sample = 16u16;

    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&(sample_rate as u32).to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&pcm);
    Ok(wav)
}

fn encode_with_ffmpeg(audio: &SynthesizedAudio, output_format: &str) -> Result<Vec<u8>> {
    let wav = float_pcm_to_wav(&audio.samples, audio.sample_rate)?;
    let mut child = Command::new("ffmpeg")
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "wav",
            "-i",
            "pipe:0",
            "-f",
            output_format,
            "pipe:1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to start ffmpeg")?;

    child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow!("failed to open ffmpeg stdin"))?
        .write_all(&wav)
        .context("failed to write wav input to ffmpeg")?;
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .context("failed to wait for ffmpeg")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ffmpeg failed to encode {output_format}: {stderr}");
    }

    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_is_little_endian_s16() {
        assert_eq!(
            float_pcm_to_s16le(&[-1.0, 0.0, 1.0]),
            vec![0x01, 0x80, 0x00, 0x00, 0xff, 0x7f]
        );
    }

    #[test]
    fn wav_has_expected_header() {
        let wav = float_pcm_to_wav(&[0.0, 0.5], 24_000).unwrap();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[12..16], b"fmt ");
        assert_eq!(&wav[36..40], b"data");
    }
}
