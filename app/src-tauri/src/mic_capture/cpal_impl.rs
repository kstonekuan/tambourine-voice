//! cpal-based microphone capture implementation using a dedicated audio thread.
//!
//! Uses channels to communicate with the audio thread since `cpal::Stream`
//! is not Send+Sync on macOS (`CoreAudio` callbacks must run on specific threads).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Sample, SampleFormat};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

use super::{AudioDeviceInfo, MicCapture, MicCaptureError};

/// Commands sent to the audio capture thread
enum AudioCommand {
    Start(Option<String>),
    Stop,
    Pause,
    Resume,
    Shutdown,
}

/// Response from the audio capture thread
enum AudioResponse {
    Started,
    Error(MicCaptureError),
}

const TARGET_OUTPUT_SAMPLE_RATE_HZ: u32 = 48_000;
/// Match the frontend worklet ring buffer size to avoid oversized writes.
const MAX_NATIVE_AUDIO_EVENT_SAMPLES: usize = 4_800;

/// Normalizes native microphone input into mono 48kHz float samples.
///
/// Handles:
/// - Channel downmixing (N channels -> mono)
/// - Sample-rate conversion (device rate -> 48kHz) using linear interpolation
struct AudioStreamNormalizer {
    input_channel_count: usize,
    input_sample_period_seconds: f64,
    output_sample_period_seconds: f64,
    current_input_time_seconds: f64,
    next_output_time_seconds: f64,
    previous_input_sample: Option<f32>,
}

impl AudioStreamNormalizer {
    fn new(input_channel_count: usize, input_sample_rate_hz: u32) -> Self {
        Self {
            input_channel_count,
            input_sample_period_seconds: 1.0 / f64::from(input_sample_rate_hz),
            output_sample_period_seconds: 1.0 / f64::from(TARGET_OUTPUT_SAMPLE_RATE_HZ),
            current_input_time_seconds: 0.0,
            next_output_time_seconds: 0.0,
            previous_input_sample: None,
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn push_mono_sample(&mut self, mono_sample: f32, normalized_output: &mut Vec<f32>) {
        if self.previous_input_sample.is_none() {
            // Emit first sample immediately so startup has deterministic, non-silent output.
            normalized_output.push(mono_sample);
            self.previous_input_sample = Some(mono_sample);
            self.next_output_time_seconds =
                self.current_input_time_seconds + self.output_sample_period_seconds;
            self.current_input_time_seconds += self.input_sample_period_seconds;
            return;
        }

        let previous_input_sample = self.previous_input_sample.unwrap_or(mono_sample);
        let previous_input_time_seconds =
            self.current_input_time_seconds - self.input_sample_period_seconds;

        while self.next_output_time_seconds <= self.current_input_time_seconds {
            let interpolation_position = ((self.next_output_time_seconds
                - previous_input_time_seconds)
                / self.input_sample_period_seconds)
                .clamp(0.0, 1.0) as f32;

            let interpolated_sample = previous_input_sample
                + (mono_sample - previous_input_sample) * interpolation_position;
            normalized_output.push(interpolated_sample);
            self.next_output_time_seconds += self.output_sample_period_seconds;
        }

        self.previous_input_sample = Some(mono_sample);
        self.current_input_time_seconds += self.input_sample_period_seconds;
    }
}

fn normalize_interleaved_input_chunk<T, F>(
    interleaved_input_samples: &[T],
    normalizer: &mut AudioStreamNormalizer,
    mut convert_sample_to_f32: F,
) -> Vec<f32>
where
    T: Copy,
    F: FnMut(T) -> f32,
{
    let mut normalized_output = Vec::new();
    let channel_count_as_f32 =
        f32::from(u16::try_from(normalizer.input_channel_count).unwrap_or(u16::MAX));

    for frame_samples in interleaved_input_samples.chunks_exact(normalizer.input_channel_count) {
        let mono_sample = frame_samples
            .iter()
            .copied()
            .map(&mut convert_sample_to_f32)
            .sum::<f32>()
            / channel_count_as_f32;

        normalizer.push_mono_sample(mono_sample, &mut normalized_output);
    }

    normalized_output
}

fn emit_normalized_audio_data_in_chunks(
    normalized_audio_data: Vec<f32>,
    on_audio_data: &Arc<dyn Fn(Vec<f32>) + Send + Sync>,
) {
    if normalized_audio_data.is_empty() {
        return;
    }

    if normalized_audio_data.len() <= MAX_NATIVE_AUDIO_EVENT_SAMPLES {
        on_audio_data(normalized_audio_data);
        return;
    }

    for normalized_audio_chunk in normalized_audio_data.chunks(MAX_NATIVE_AUDIO_EVENT_SAMPLES) {
        on_audio_data(normalized_audio_chunk.to_vec());
    }
}

fn convert_i16_sample_to_normalized_f32(sample: i16) -> f32 {
    if sample == i16::MIN {
        -1.0
    } else {
        f32::from(sample) / f32::from(i16::MAX)
    }
}

fn convert_u16_sample_to_normalized_f32(sample: u16) -> f32 {
    let signed_sample_centered_at_zero = f32::from(sample) - 32_768.0;
    signed_sample_centered_at_zero / 32_768.0
}

fn convert_f32_sample_to_normalized_f32(sample: f32) -> f32 {
    sample
}

fn convert_sample_to_normalized_f32<T>(sample: T) -> f32
where
    f32: cpal::FromSample<T>,
{
    f32::from_sample(sample)
}

fn build_normalized_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    input_channel_count: usize,
    input_sample_rate_hz: u32,
    is_paused: &Arc<AtomicBool>,
    on_audio_data: &Arc<dyn Fn(Vec<f32>) + Send + Sync>,
    convert_sample_to_f32: fn(T) -> f32,
) -> Result<cpal::Stream, MicCaptureError>
where
    T: cpal::SizedSample + Copy + Send + 'static,
{
    let is_paused_for_callback = Arc::clone(is_paused);
    let on_audio_data_for_callback = Arc::clone(on_audio_data);
    let mut audio_stream_normalizer =
        AudioStreamNormalizer::new(input_channel_count, input_sample_rate_hz);

    device
        .build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if is_paused_for_callback.load(Ordering::Relaxed) {
                    return;
                }

                let normalized_audio_data = normalize_interleaved_input_chunk(
                    data,
                    &mut audio_stream_normalizer,
                    convert_sample_to_f32,
                );
                emit_normalized_audio_data_in_chunks(
                    normalized_audio_data,
                    &on_audio_data_for_callback,
                );
            },
            |err| log::error!("Audio stream error: {err}"),
            None,
        )
        .map_err(|error| MicCaptureError::StreamCreationFailed(error.to_string()))
}

pub struct CpalMicCapture {
    command_tx: Sender<AudioCommand>,
    /// Wrapped in Mutex to make the struct Sync (required by Tauri's state management)
    response_rx: Mutex<Receiver<AudioResponse>>,
    /// Thread handle for proper shutdown - wrapped in Mutex for Sync
    thread_handle: Mutex<Option<thread::JoinHandle<()>>>,
}

impl CpalMicCapture {
    pub fn new<F>(on_audio_data: F) -> Self
    where
        F: Fn(Vec<f32>) + Send + Sync + 'static,
    {
        let (command_tx, command_rx) = channel::<AudioCommand>();
        let (response_tx, response_rx) = channel::<AudioResponse>();

        let on_audio_data = Arc::new(on_audio_data);

        // Spawn dedicated audio thread
        let thread_handle = thread::spawn(move || {
            let mut current_stream: Option<cpal::Stream> = None;
            // Track paused state locally in the audio thread
            let is_paused = Arc::new(AtomicBool::new(false));

            loop {
                match command_rx.recv() {
                    Ok(AudioCommand::Start(device_id)) => {
                        let start_time = std::time::Instant::now();

                        // Stop existing stream
                        current_stream.take();

                        match create_stream(
                            device_id.as_deref(),
                            is_paused.clone(),
                            on_audio_data.clone(),
                        ) {
                            Ok(stream) => {
                                current_stream = Some(stream);
                                is_paused.store(false, Ordering::SeqCst);
                                let _ = response_tx.send(AudioResponse::Started);
                                log::info!(
                                    "Native mic capture started in {:?}",
                                    start_time.elapsed()
                                );
                            }
                            Err(e) => {
                                log::error!(
                                    "Native mic capture failed after {:?}: {}",
                                    start_time.elapsed(),
                                    e
                                );
                                let _ = response_tx.send(AudioResponse::Error(e));
                            }
                        }
                    }
                    Ok(AudioCommand::Stop) => {
                        current_stream.take();
                        log::info!("Native mic capture stopped");
                    }
                    Ok(AudioCommand::Pause) => {
                        is_paused.store(true, Ordering::SeqCst);
                        log::debug!("Native mic capture paused");
                    }
                    Ok(AudioCommand::Resume) => {
                        is_paused.store(false, Ordering::SeqCst);
                        log::debug!("Native mic capture resumed");
                    }
                    Ok(AudioCommand::Shutdown) | Err(_) => {
                        current_stream.take();
                        log::info!("Audio capture thread shutting down");
                        break;
                    }
                }
            }
        });

        Self {
            command_tx,
            response_rx: Mutex::new(response_rx),
            thread_handle: Mutex::new(Some(thread_handle)),
        }
    }
}

/// Create an audio input stream (runs on the audio thread)
#[allow(clippy::too_many_lines)]
fn create_stream(
    device_id: Option<&str>,
    is_paused: Arc<AtomicBool>,
    on_audio_data: Arc<dyn Fn(Vec<f32>) + Send + Sync>,
) -> Result<cpal::Stream, MicCaptureError> {
    let host = cpal::default_host();

    let device = match device_id {
        Some(id) => host
            .input_devices()
            .map_err(|e| MicCaptureError::DeviceNotFound(e.to_string()))?
            .find(|d| {
                d.id()
                    .map(|dev_id| dev_id.to_string() == id)
                    .unwrap_or(false)
            })
            .ok_or_else(|| MicCaptureError::DeviceNotFound(id.to_string()))?,
        None => host
            .default_input_device()
            .ok_or_else(|| MicCaptureError::DeviceNotFound("No default device".into()))?,
    };

    // Use the device's default config for compatibility; normalize in software.
    let default_config = device
        .default_input_config()
        .map_err(|e| MicCaptureError::StreamCreationFailed(e.to_string()))?;
    let config = default_config.config();
    let input_channel_count = usize::from(config.channels);
    let input_sample_rate_hz = config.sample_rate;
    let input_sample_format = default_config.sample_format();

    log::info!(
        "Using native input config: {}Hz, {} channel(s), {:?} format; normalizing to {}Hz mono",
        config.sample_rate,
        config.channels,
        input_sample_format,
        TARGET_OUTPUT_SAMPLE_RATE_HZ
    );

    let stream = match input_sample_format {
        SampleFormat::I8 => build_normalized_input_stream::<i8>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<i8>,
        )?,
        SampleFormat::I16 => build_normalized_input_stream::<i16>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_i16_sample_to_normalized_f32,
        )?,
        SampleFormat::I24 => build_normalized_input_stream::<cpal::I24>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<cpal::I24>,
        )?,
        SampleFormat::I32 => build_normalized_input_stream::<i32>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<i32>,
        )?,
        SampleFormat::I64 => build_normalized_input_stream::<i64>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<i64>,
        )?,
        SampleFormat::U8 => build_normalized_input_stream::<u8>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<u8>,
        )?,
        SampleFormat::U16 => build_normalized_input_stream::<u16>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_u16_sample_to_normalized_f32,
        )?,
        SampleFormat::U24 => build_normalized_input_stream::<cpal::U24>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<cpal::U24>,
        )?,
        SampleFormat::U32 => build_normalized_input_stream::<u32>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<u32>,
        )?,
        SampleFormat::U64 => build_normalized_input_stream::<u64>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<u64>,
        )?,
        SampleFormat::F32 => build_normalized_input_stream::<f32>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_f32_sample_to_normalized_f32,
        )?,
        SampleFormat::F64 => build_normalized_input_stream::<f64>(
            &device,
            &config,
            input_channel_count,
            input_sample_rate_hz,
            &is_paused,
            &on_audio_data,
            convert_sample_to_normalized_f32::<f64>,
        )?,
        _ => {
            return Err(MicCaptureError::StreamCreationFailed(format!(
                "Unsupported input sample format: {input_sample_format:?}"
            )));
        }
    };

    stream
        .play()
        .map_err(|e| MicCaptureError::StreamStartFailed(e.to_string()))?;

    Ok(stream)
}

impl MicCapture for CpalMicCapture {
    fn start(&self, device_id: Option<&str>) -> Result<(), MicCaptureError> {
        let _ = self
            .command_tx
            .send(AudioCommand::Start(device_id.map(String::from)));

        // Wait for response with timeout
        let rx = self.response_rx.lock().unwrap();
        match rx.recv_timeout(std::time::Duration::from_secs(5)) {
            Ok(AudioResponse::Started) => Ok(()),
            Ok(AudioResponse::Error(e)) => Err(e),
            Err(_) => Err(MicCaptureError::StreamStartFailed(
                "Timeout waiting for audio thread".into(),
            )),
        }
    }

    fn pause(&self) {
        let _ = self.command_tx.send(AudioCommand::Pause);
    }

    fn resume(&self) {
        let _ = self.command_tx.send(AudioCommand::Resume);
    }

    fn stop(&self) {
        let _ = self.command_tx.send(AudioCommand::Stop);
    }

    fn list_devices(&self) -> Vec<AudioDeviceInfo> {
        let host = cpal::default_host();
        host.input_devices()
            .map(|devices| {
                devices
                    .filter_map(|d| {
                        let id = d.id().ok()?.to_string();
                        let name = d.description().ok()?.name().to_string();
                        Some(AudioDeviceInfo { id, name })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Drop for CpalMicCapture {
    fn drop(&mut self) {
        // Send shutdown command to the audio thread
        let _ = self.command_tx.send(AudioCommand::Shutdown);

        // Wait for the thread to finish (proper cleanup)
        if let Some(handle) = self.thread_handle.lock().unwrap().take() {
            if let Err(e) = handle.join() {
                log::error!("Audio capture thread panicked: {e:?}");
            }
        }
    }
}

#[cfg(test)]
#[path = "../tests/mic_capture_cpal_impl_tests.rs"]
mod mic_capture_cpal_impl_tests;
