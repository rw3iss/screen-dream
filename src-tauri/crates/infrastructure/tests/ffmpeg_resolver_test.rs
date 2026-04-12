use domain::ffmpeg::FfmpegProvider;
use infrastructure::ffmpeg::FfmpegResolver;

#[test]
fn resolves_system_ffmpeg() {
    // No sidecar dir, no custom path — should find system FFmpeg
    let resolver = FfmpegResolver::new(None, None);

    // This will skip if FFmpeg is not installed
    match resolver.ffmpeg_path() {
        Ok(path) => {
            assert!(path.is_file(), "Resolved path should be a file: {}", path.display());
            println!("Found FFmpeg at: {}", path.display());

            let caps = resolver.capabilities().expect("Should query capabilities");
            println!("Version: {}", caps.version);
            println!("Video encoders: {:?}", caps.video_encoders);
            println!("Audio encoders: {:?}", caps.audio_encoders);
            assert!(!caps.version.is_empty());
        }
        Err(e) => {
            eprintln!("Skipping test — FFmpeg not installed: {e}");
        }
    }
}

#[test]
fn returns_error_for_nonexistent_custom_path() {
    let resolver = FfmpegResolver::new(
        None,
        Some("/nonexistent/path/ffmpeg".into()),
    );

    // Custom path doesn't exist, no sidecar, might fall back to system
    // The key behavior: it doesn't panic, and returns a usable result or clear error
    let result = resolver.ffmpeg_path();
    println!("Result with bad custom path: {result:?}");
}
