//! Windows audio mute control implementation using WASAPI.
//!
//! Uses the Windows Audio Session API (WASAPI) to control the default audio
//! output device's mute state.

use super::{AudioControlError, SystemAudioControl};
use windows::Win32::{
    Media::Audio::{
        eConsole, eRender, Endpoints::IAudioEndpointVolume, IMMDevice, IMMDeviceEnumerator,
        MMDeviceEnumerator,
    },
    System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
};

/// Windows audio controller using WASAPI.
pub struct WindowsAudioController {
    endpoint_volume: IAudioEndpointVolume,
}

// SAFETY: IAudioEndpointVolume is thread-safe when properly initialized with COM
unsafe impl Send for WindowsAudioController {}
unsafe impl Sync for WindowsAudioController {}

impl WindowsAudioController {
    /// Create a new Windows audio controller.
    ///
    /// Initializes COM and gets the default audio endpoint volume control.
    pub fn new() -> Result<Self, AudioControlError> {
        unsafe {
            // Initialize COM (ignore error if already initialized)
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            // Create device enumerator
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|e| {
                    AudioControlError::InitializationFailed(format!(
                        "Failed to create device enumerator: {e}"
                    ))
                })?;

            // Get default audio output device
            let device: IMMDevice = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| {
                    AudioControlError::InitializationFailed(format!(
                        "Failed to get default audio endpoint: {e}"
                    ))
                })?;

            // Get the endpoint volume interface
            let endpoint_volume = device
                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                .map_err(|e| {
                    AudioControlError::InitializationFailed(format!(
                        "Failed to activate endpoint volume: {e}"
                    ))
                })?;

            Ok(Self { endpoint_volume })
        }
    }
}

impl SystemAudioControl for WindowsAudioController {
    fn is_muted(&self) -> Result<bool, AudioControlError> {
        unsafe {
            self.endpoint_volume
                .GetMute()
                .map(windows::core::BOOL::as_bool)
                .map_err(|e| AudioControlError::GetPropertyFailed(format!("GetMute: {e}")))
        }
    }

    fn set_muted(&self, muted: bool) -> Result<(), AudioControlError> {
        unsafe {
            self.endpoint_volume
                .SetMute(muted, std::ptr::null())
                .map_err(|e| AudioControlError::SetPropertyFailed(format!("SetMute: {e}")))
        }
    }
}
