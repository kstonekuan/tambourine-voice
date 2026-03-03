//! Windows audio mute control implementation using WASAPI.
//!
//! Uses the Windows Audio Session API (WASAPI) to control the default audio
//! output device's mute state.

use super::{ActiveMuteSession, AudioControlError, SystemAudioControl};
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
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).map_err(|error| {
                    AudioControlError::InitializationFailed(format!(
                        "Failed to create device enumerator: {error}"
                    ))
                })?;

            // Get default audio output device
            let device: IMMDevice = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|error| {
                    AudioControlError::InitializationFailed(format!(
                        "Failed to get default audio endpoint: {error}"
                    ))
                })?;

            // Get the endpoint volume interface
            let endpoint_volume = device
                .Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
                .map_err(|error| {
                    AudioControlError::InitializationFailed(format!(
                        "Failed to activate endpoint volume: {error}"
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
                .map_err(|error| AudioControlError::GetPropertyFailed(format!("GetMute: {error}")))
        }
    }

    fn begin_mute_session(&self) -> Result<ActiveMuteSession, AudioControlError> {
        unsafe {
            self.endpoint_volume
                .SetMute(true, std::ptr::null())
                .map_err(|error| {
                    AudioControlError::SetPropertyFailed(format!("SetMute: {error}"))
                })?;
        }

        Ok(ActiveMuteSession::WindowsEndpointMute)
    }

    fn end_mute_session(
        &self,
        active_mute_session: &ActiveMuteSession,
    ) -> Result<(), AudioControlError> {
        if !matches!(active_mute_session, ActiveMuteSession::WindowsEndpointMute) {
            return Err(AudioControlError::SetPropertyFailed(
                "Received non-Windows mute session in Windows audio controller".to_string(),
            ));
        }

        unsafe {
            self.endpoint_volume
                .SetMute(false, std::ptr::null())
                .map_err(|error| AudioControlError::SetPropertyFailed(format!("SetMute: {error}")))
        }
    }
}
