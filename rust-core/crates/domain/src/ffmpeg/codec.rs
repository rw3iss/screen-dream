use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    H264,
    H265,
    Vp9,
    Av1,
}

impl VideoCodec {
    /// The FFmpeg encoder name for this codec.
    pub fn encoder_name(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::Vp9 => "libvpx-vp9",
            VideoCodec::Av1 => "libaom-av1",
        }
    }

    /// The FFmpeg codec flag used in `-codecs` output.
    pub fn probe_name(&self) -> &'static str {
        match self {
            VideoCodec::H264 => "h264",
            VideoCodec::H265 => "hevc",
            VideoCodec::Vp9 => "vp9",
            VideoCodec::Av1 => "av1",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    Aac,
    Opus,
    Mp3,
}

impl AudioCodec {
    pub fn encoder_name(&self) -> &'static str {
        match self {
            AudioCodec::Aac => "aac",
            AudioCodec::Opus => "libopus",
            AudioCodec::Mp3 => "libmp3lame",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ContainerFormat {
    Mp4,
    Webm,
    Mkv,
    Gif,
}

impl ContainerFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ContainerFormat::Mp4 => "mp4",
            ContainerFormat::Webm => "webm",
            ContainerFormat::Mkv => "mkv",
            ContainerFormat::Gif => "gif",
        }
    }

    pub fn ffmpeg_format(&self) -> &'static str {
        match self {
            ContainerFormat::Mp4 => "mp4",
            ContainerFormat::Webm => "webm",
            ContainerFormat::Mkv => "matroska",
            ContainerFormat::Gif => "gif",
        }
    }
}

/// Describes what an FFmpeg installation can do.
#[derive(Debug, Clone, Serialize)]
pub struct FfmpegCapabilities {
    pub version: String,
    pub video_encoders: Vec<VideoCodec>,
    pub audio_encoders: Vec<AudioCodec>,
}
