use super::codec::{AudioCodec, ContainerFormat, VideoCodec};

/// A builder for constructing FFmpeg command-line arguments.
/// This is a pure domain object -- it does not spawn processes.
/// The infrastructure layer takes the built args and executes them.
#[derive(Debug, Clone)]
pub struct FfmpegCommand {
    args: Vec<String>,
}

impl FfmpegCommand {
    pub fn new() -> Self {
        FfmpegCommand {
            args: vec!["-hide_banner".to_string()],
        }
    }

    /// Add `-y` to overwrite output without asking.
    pub fn overwrite(mut self) -> Self {
        self.args.push("-y".to_string());
        self
    }

    /// Add an input file: `-i <path>`
    pub fn input(mut self, path: &str) -> Self {
        self.args.push("-i".to_string());
        self.args.push(path.to_string());
        self
    }

    /// Read from stdin pipe: `-i pipe:0`
    pub fn input_pipe(mut self) -> Self {
        self.args.push("-i".to_string());
        self.args.push("pipe:0".to_string());
        self
    }

    /// Set video codec: `-c:v <encoder>`
    pub fn video_codec(mut self, codec: &VideoCodec) -> Self {
        self.args.push("-c:v".to_string());
        self.args.push(codec.encoder_name().to_string());
        self
    }

    /// Set audio codec: `-c:a <encoder>`
    pub fn audio_codec(mut self, codec: &AudioCodec) -> Self {
        self.args.push("-c:a".to_string());
        self.args.push(codec.encoder_name().to_string());
        self
    }

    /// Set output format: `-f <format>`
    pub fn format(mut self, fmt: &ContainerFormat) -> Self {
        self.args.push("-f".to_string());
        self.args.push(fmt.ffmpeg_format().to_string());
        self
    }

    /// Set video framerate: `-r <fps>`
    pub fn framerate(mut self, fps: u32) -> Self {
        self.args.push("-r".to_string());
        self.args.push(fps.to_string());
        self
    }

    /// Set video resolution: `-s <width>x<height>`
    pub fn resolution(mut self, width: u32, height: u32) -> Self {
        self.args.push("-s".to_string());
        self.args.push(format!("{width}x{height}"));
        self
    }

    /// Set pixel format: `-pix_fmt <fmt>`
    pub fn pixel_format(mut self, fmt: &str) -> Self {
        self.args.push("-pix_fmt".to_string());
        self.args.push(fmt.to_string());
        self
    }

    /// Add a video filter: `-vf <filter>`
    pub fn video_filter(mut self, filter: &str) -> Self {
        self.args.push("-vf".to_string());
        self.args.push(filter.to_string());
        self
    }

    /// Add an audio filter: `-af <filter>`
    pub fn audio_filter(mut self, filter: &str) -> Self {
        self.args.push("-af".to_string());
        self.args.push(filter.to_string());
        self
    }

    /// Add a complex filter graph: `-filter_complex <graph>`
    pub fn filter_complex(mut self, graph: &str) -> Self {
        self.args.push("-filter_complex".to_string());
        self.args.push(graph.to_string());
        self
    }

    /// Set CRF quality: `-crf <value>` (0=lossless, 23=default, 51=worst)
    pub fn crf(mut self, value: u8) -> Self {
        self.args.push("-crf".to_string());
        self.args.push(value.to_string());
        self
    }

    /// Set encoding preset: `-preset <preset>`
    pub fn preset(mut self, preset: &str) -> Self {
        self.args.push("-preset".to_string());
        self.args.push(preset.to_string());
        self
    }

    /// Add an arbitrary argument.
    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    /// Set the output file path (must be last).
    pub fn output(mut self, path: &str) -> Self {
        self.args.push(path.to_string());
        self
    }

    /// Output to stdout pipe: `pipe:1`
    pub fn output_pipe(mut self) -> Self {
        self.args.push("pipe:1".to_string());
        self
    }

    /// Return the assembled argument list.
    pub fn build(self) -> Vec<String> {
        self.args
    }
}

impl Default for FfmpegCommand {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_simple_transcode_command() {
        let args = FfmpegCommand::new()
            .overwrite()
            .input("input.webm")
            .video_codec(&VideoCodec::H264)
            .audio_codec(&AudioCodec::Aac)
            .crf(23)
            .preset("fast")
            .output("output.mp4")
            .build();

        assert_eq!(
            args,
            vec![
                "-hide_banner",
                "-y",
                "-i",
                "input.webm",
                "-c:v",
                "libx264",
                "-c:a",
                "aac",
                "-crf",
                "23",
                "-preset",
                "fast",
                "output.mp4",
            ]
        );
    }

    #[test]
    fn builds_pipe_input_command() {
        let args = FfmpegCommand::new()
            .overwrite()
            .arg("-f")
            .arg("rawvideo")
            .pixel_format("bgra")
            .resolution(1920, 1080)
            .framerate(30)
            .input_pipe()
            .video_codec(&VideoCodec::H264)
            .crf(18)
            .preset("ultrafast")
            .output("recording.mp4")
            .build();

        assert!(args.contains(&"pipe:0".to_string()));
        assert!(args.contains(&"1920x1080".to_string()));
        assert!(args.contains(&"30".to_string()));
    }

    #[test]
    fn builds_video_filter_command() {
        let args = FfmpegCommand::new()
            .input("input.mp4")
            .video_filter("crop=1280:720:0:0")
            .output("cropped.mp4")
            .build();

        assert!(args.contains(&"-vf".to_string()));
        assert!(args.contains(&"crop=1280:720:0:0".to_string()));
    }
}
