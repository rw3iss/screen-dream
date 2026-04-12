use std::path::Path;
use std::process::Command;

use domain::error::{AppError, AppResult};
use domain::ffmpeg::codec::{AudioCodec, FfmpegCapabilities, VideoCodec};
use tracing::debug;

/// Runs `ffmpeg -version` and `ffmpeg -encoders` to discover capabilities.
pub fn query_capabilities(ffmpeg_path: &Path) -> AppResult<FfmpegCapabilities> {
    let version = query_version(ffmpeg_path)?;
    let video_encoders = detect_video_encoders(ffmpeg_path)?;
    let audio_encoders = detect_audio_encoders(ffmpeg_path)?;

    Ok(FfmpegCapabilities {
        version,
        video_encoders,
        audio_encoders,
    })
}

fn query_version(ffmpeg_path: &Path) -> AppResult<String> {
    let output = Command::new(ffmpeg_path)
        .args(["-version"])
        .output()
        .map_err(|e| AppError::FfmpegExecution(format!("Failed to run ffmpeg -version: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    // First line looks like: "ffmpeg version 6.1.1 Copyright ..."
    let version_line = stdout.lines().next().unwrap_or("unknown");
    let version = version_line
        .strip_prefix("ffmpeg version ")
        .unwrap_or(version_line)
        .split_whitespace()
        .next()
        .unwrap_or("unknown")
        .to_string();

    debug!("FFmpeg version: {version}");
    Ok(version)
}

fn detect_video_encoders(ffmpeg_path: &Path) -> AppResult<Vec<VideoCodec>> {
    let output = Command::new(ffmpeg_path)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|e| AppError::FfmpegExecution(format!("Failed to run ffmpeg -encoders: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut codecs = Vec::new();

    let candidates = [
        ("libx264", VideoCodec::H264),
        ("libx265", VideoCodec::H265),
        ("libvpx-vp9", VideoCodec::Vp9),
        ("libaom-av1", VideoCodec::Av1),
    ];

    for (name, codec) in &candidates {
        if stdout.contains(name) {
            codecs.push(codec.clone());
        }
    }

    debug!("Detected video encoders: {codecs:?}");
    Ok(codecs)
}

fn detect_audio_encoders(ffmpeg_path: &Path) -> AppResult<Vec<AudioCodec>> {
    let output = Command::new(ffmpeg_path)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|e| AppError::FfmpegExecution(format!("Failed to run ffmpeg -encoders: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut codecs = Vec::new();

    let candidates = [
        ("aac", AudioCodec::Aac),
        ("libopus", AudioCodec::Opus),
        ("libmp3lame", AudioCodec::Mp3),
    ];

    for (name, codec) in &candidates {
        // Check for the encoder name specifically (avoid substring matches)
        // The output format is: " V..... libx264 ..."
        if stdout.lines().any(|line| {
            line.split_whitespace()
                .nth(1)
                .map_or(false, |encoder| encoder == *name)
        }) {
            codecs.push(codec.clone());
        }
    }

    // AAC is built-in to most FFmpeg builds (not just libfdk_aac)
    if !codecs.iter().any(|c| matches!(c, AudioCodec::Aac))
        && stdout.contains(" aac ")
    {
        codecs.push(AudioCodec::Aac);
    }

    debug!("Detected audio encoders: {codecs:?}");
    Ok(codecs)
}
