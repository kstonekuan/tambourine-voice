//! Native microphone capture for low-latency audio.
//!
//! Captures audio directly via `CoreAudio` (macOS) / WASAPI (Windows) using cpal,
//! then streams PCM data to the frontend via Tauri events.

use serde::Serialize;
use std::sync::Arc;

mod cpal_impl;

pub use cpal_impl::CpalMicCapture;

/// Information about an audio input device
#[derive(Debug, Clone, Serialize)]
pub struct AudioDeviceInfo {
    /// Stable unique identifier (persists across reboots)
    pub id: String,
    /// Human-readable device name for display
    pub name: String,
}

/// Error type for mic capture operations
#[derive(Debug, Clone)]
pub enum MicCaptureError {
    DeviceNotFound(String),
    StreamCreationFailed(String),
    StreamStartFailed(String),
}

impl std::fmt::Display for MicCaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound(msg) => write!(f, "Mic device not found: {msg}"),
            Self::StreamCreationFailed(msg) => write!(f, "Failed to create stream: {msg}"),
            Self::StreamStartFailed(msg) => write!(f, "Failed to start stream: {msg}"),
        }
    }
}

impl std::error::Error for MicCaptureError {}

/// Trait for native microphone capture
pub trait MicCapture: Send + Sync {
    /// Start capturing audio from the specified device (by ID)
    fn start(&self, device_id: Option<&str>) -> Result<(), MicCaptureError>;

    /// Pause capture (keeps stream alive for instant resume)
    fn pause(&self);

    /// Resume capture after pause
    fn resume(&self);

    /// Stop capture and release resources
    fn stop(&self);

    /// List available input devices with ID and name
    fn list_devices(&self) -> Vec<AudioDeviceInfo>;
}

/// Manages mic capture lifecycle with proper cleanup
pub struct MicCaptureManager {
    capture: Arc<CpalMicCapture>,
}

impl MicCaptureManager {
    pub fn new<F>(on_audio_data: F) -> Self
    where
        F: Fn(Vec<f32>) + Send + Sync + 'static,
    {
        Self {
            capture: Arc::new(CpalMicCapture::new(on_audio_data)),
        }
    }

    pub fn capture(&self) -> &Arc<CpalMicCapture> {
        &self.capture
    }
}

impl Drop for MicCaptureManager {
    fn drop(&mut self) {
        self.capture.stop();
    }
}
