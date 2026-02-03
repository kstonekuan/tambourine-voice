//! cpal-based microphone capture implementation using a dedicated audio thread.
//!
//! Uses channels to communicate with the audio thread since `cpal::Stream`
//! is not Send+Sync on macOS (`CoreAudio` callbacks must run on specific threads).

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
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

    // Use the device's default config - it's guaranteed to work
    let default_config = device
        .default_input_config()
        .map_err(|e| MicCaptureError::StreamCreationFailed(e.to_string()))?;

    let config = cpal::StreamConfig {
        channels: default_config.channels().min(2), // Use mono or stereo max
        sample_rate: default_config.sample_rate(),
        buffer_size: cpal::BufferSize::Default,
    };

    log::info!(
        "Using device audio config: {}Hz, {} channel(s)",
        config.sample_rate,
        config.channels
    );

    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !is_paused.load(Ordering::Relaxed) {
                    on_audio_data(data.to_vec());
                }
            },
            |err| log::error!("Audio stream error: {err}"),
            None,
        )
        .map_err(|e| MicCaptureError::StreamCreationFailed(e.to_string()))?;

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
