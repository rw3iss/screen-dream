use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, SampleFormat, SampleRate, StreamConfig};
use domain::error::{AppError, AppResult};
use tracing::{debug, error, info};

/// Information about an available audio input device.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
    pub sample_rate: u32,
    pub channels: u16,
}

/// Manages microphone audio capture, writing samples to a WAV file.
pub struct AudioCapture {
    /// Shared flag: set to false to stop recording.
    running: Arc<AtomicBool>,
    /// The cpal stream handle (keeps the stream alive while held).
    stream: Option<cpal::Stream>,
    /// Path to the output WAV file.
    output_path: PathBuf,
    /// WAV writer wrapped in Arc<Mutex> for thread-safe access from the audio callback.
    writer: Arc<Mutex<Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>>>,
    /// Sample rate of the recording.
    pub sample_rate: u32,
    /// Number of channels.
    pub channels: u16,
}

/// List available audio input devices.
pub fn list_audio_devices() -> AppResult<Vec<AudioDeviceInfo>> {
    let host = cpal::default_host();

    let default_device_name = host
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_default();

    let mut devices = Vec::new();

    let input_devices = host.input_devices().map_err(|e| {
        AppError::Capture(format!("Failed to list audio input devices: {e}"))
    })?;

    for device in input_devices {
        let name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        let is_default = name == default_device_name;

        let config = device.default_input_config().map_err(|e| {
            AppError::Capture(format!("Failed to get config for device {name}: {e}"))
        })?;

        devices.push(AudioDeviceInfo {
            name,
            is_default,
            sample_rate: config.sample_rate().0,
            channels: config.channels(),
        });
    }

    debug!("Found {} audio input devices", devices.len());
    Ok(devices)
}

impl AudioCapture {
    /// Start capturing audio from the specified device (or default) to a WAV file.
    pub fn start(device_name: Option<&str>, output_path: PathBuf) -> AppResult<Self> {
        let host = cpal::default_host();

        // Find the target device.
        let device: Device = if let Some(name) = device_name {
            let input_devices = host.input_devices().map_err(|e| {
                AppError::Capture(format!("Failed to list input devices: {e}"))
            })?;

            input_devices
                .into_iter()
                .find(|d| d.name().map_or(false, |n| n == name))
                .ok_or_else(|| {
                    AppError::Capture(format!("Audio device not found: {name}"))
                })?
        } else {
            host.default_input_device().ok_or_else(|| {
                AppError::Capture("No default audio input device available".to_string())
            })?
        };

        let device_name = device.name().unwrap_or_else(|_| "Unknown".to_string());
        info!("Using audio device: {device_name}");

        let supported_config = device.default_input_config().map_err(|e| {
            AppError::Capture(format!("Failed to get input config: {e}"))
        })?;

        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels();
        let sample_format = supported_config.sample_format();

        info!(
            "Audio config: {}Hz, {} channels, {:?}",
            sample_rate, channels, sample_format
        );

        // Create the WAV writer.
        let wav_spec = hound::WavSpec {
            channels,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let writer = hound::WavWriter::create(&output_path, wav_spec).map_err(|e| {
            AppError::Capture(format!(
                "Failed to create WAV file at {}: {e}",
                output_path.display()
            ))
        })?;

        let writer = Arc::new(Mutex::new(Some(writer)));
        let running = Arc::new(AtomicBool::new(true));

        let writer_clone = writer.clone();
        let running_clone = running.clone();

        let stream_config = StreamConfig {
            channels,
            sample_rate: SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let err_callback = |err: cpal::StreamError| {
            error!("Audio stream error: {err}");
        };

        // Build the input stream based on sample format.
        let stream = match sample_format {
            SampleFormat::F32 => device.build_input_stream(
                &stream_config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if !running_clone.load(Ordering::Relaxed) {
                        return;
                    }
                    if let Ok(mut guard) = writer_clone.lock() {
                        if let Some(ref mut w) = *guard {
                            for &sample in data {
                                if w.write_sample(sample).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                },
                err_callback,
                None,
            ),
            SampleFormat::I16 => {
                let writer_clone2 = writer.clone();
                let running_clone2 = running.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !running_clone2.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut guard) = writer_clone2.lock() {
                            if let Some(ref mut w) = *guard {
                                for &sample in data {
                                    let f = sample as f32 / i16::MAX as f32;
                                    if w.write_sample(f).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            SampleFormat::U16 => {
                let writer_clone3 = writer.clone();
                let running_clone3 = running.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if !running_clone3.load(Ordering::Relaxed) {
                            return;
                        }
                        if let Ok(mut guard) = writer_clone3.lock() {
                            if let Some(ref mut w) = *guard {
                                for &sample in data {
                                    let f = (sample as f32 - 32768.0) / 32768.0;
                                    if w.write_sample(f).is_err() {
                                        break;
                                    }
                                }
                            }
                        }
                    },
                    err_callback,
                    None,
                )
            }
            other => {
                return Err(AppError::Capture(format!(
                    "Unsupported audio sample format: {other:?}"
                )));
            }
        }
        .map_err(|e| {
            AppError::Capture(format!("Failed to build audio input stream: {e}"))
        })?;

        // Start the stream.
        stream.play().map_err(|e| {
            AppError::Capture(format!("Failed to start audio stream: {e}"))
        })?;

        info!("Audio capture started -> {}", output_path.display());

        Ok(AudioCapture {
            running,
            stream: Some(stream),
            output_path,
            writer,
            sample_rate,
            channels,
        })
    }

    /// Stop audio capture and finalize the WAV file.
    /// Returns the path to the WAV file.
    pub fn stop(&mut self) -> AppResult<PathBuf> {
        info!("Stopping audio capture");
        self.running.store(false, Ordering::SeqCst);

        // Drop the stream to stop the audio callback.
        self.stream.take();

        // Finalize the WAV file.
        if let Ok(mut guard) = self.writer.lock() {
            if let Some(writer) = guard.take() {
                writer.finalize().map_err(|e| {
                    AppError::Capture(format!("Failed to finalize WAV file: {e}"))
                })?;
            }
        }

        info!("Audio saved to {}", self.output_path.display());
        Ok(self.output_path.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_audio_devices_does_not_panic() {
        // May return empty list if no audio devices, but should not panic.
        match list_audio_devices() {
            Ok(devices) => {
                println!("Audio devices: {:#?}", devices);
            }
            Err(e) => {
                eprintln!("Audio device listing failed (no audio subsystem?): {e}");
            }
        }
    }

    // Manual test: requires a microphone.
    // cargo test -p infrastructure audio_capture_record -- --nocapture --ignored
    #[test]
    #[ignore]
    fn record_two_seconds_audio() {
        let output = PathBuf::from("/tmp/test-audio.wav");
        let mut capture = AudioCapture::start(None, output.clone()).expect("start audio");

        std::thread::sleep(std::time::Duration::from_secs(2));

        let path = capture.stop().expect("stop audio");
        assert_eq!(path, output);

        let metadata = std::fs::metadata(&path).expect("WAV file should exist");
        assert!(metadata.len() > 44, "WAV file should have data beyond header");

        println!("Audio recorded: {} ({} bytes)", path.display(), metadata.len());
    }
}
