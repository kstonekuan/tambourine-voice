//! macOS audio mute control implementation using `CoreAudio`.
//!
//! Uses the `CoreAudio` framework to control the default audio output device's
//! mute state via `AudioObject` property APIs.

use super::{ActiveMuteSession, AudioControlError, OutputVolumeScalarSnapshot, SystemAudioControl};
use objc2_core_audio::{
    kAudioDevicePropertyMute, kAudioDevicePropertyPreferredChannelsForStereo,
    kAudioDevicePropertyScopeOutput, kAudioDevicePropertyVolumeScalar,
    kAudioHardwarePropertyDefaultOutputDevice, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectGetPropertyData,
    AudioObjectHasProperty, AudioObjectIsPropertySettable, AudioObjectPropertyAddress,
    AudioObjectSetPropertyData,
};
use std::ffi::c_void;
use std::ptr::NonNull;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MuteStrategy {
    DeviceMuteProperty,
    VolumeZeroFallback,
}

/// macOS audio controller using `CoreAudio`.
pub struct MacOSAudioController;

// SAFETY: CoreAudio APIs are thread-safe
unsafe impl Send for MacOSAudioController {}
unsafe impl Sync for MacOSAudioController {}

impl MacOSAudioController {
    /// Create a new macOS audio controller.
    pub fn new() -> Result<Self, AudioControlError> {
        let _ = Self::get_default_output_device()?;
        Ok(Self)
    }

    /// Get the default audio output device ID.
    fn get_default_output_device() -> Result<u32, AudioControlError> {
        let address = AudioObjectPropertyAddress {
            mSelector: kAudioHardwarePropertyDefaultOutputDevice,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };

        let mut device_id: u32 = 0;
        let mut size = u32::try_from(std::mem::size_of::<u32>()).map_err(|_| {
            AudioControlError::InitializationFailed(
                "Failed to convert output device payload size to CoreAudio size type".to_string(),
            )
        })?;

        let status = unsafe {
            AudioObjectGetPropertyData(
                kAudioObjectSystemObject as u32,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
                0,
                std::ptr::null(),
                NonNull::new(&raw mut size).unwrap(),
                NonNull::new((&raw mut device_id).cast::<c_void>()).unwrap(),
            )
        };

        if status != 0 {
            return Err(AudioControlError::InitializationFailed(format!(
                "Failed to get default output device (OSStatus: {status})"
            )));
        }

        if device_id == 0 {
            return Err(AudioControlError::InitializationFailed(
                "No default output device found".to_string(),
            ));
        }

        Ok(device_id)
    }

    fn build_output_property_address(selector: u32, element: u32) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioDevicePropertyScopeOutput,
            mElement: element,
        }
    }

    fn has_property(device_id: u32, selector: u32, element: u32) -> bool {
        let address = Self::build_output_property_address(selector, element);
        unsafe {
            AudioObjectHasProperty(
                device_id,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
            )
        }
    }

    fn is_property_settable(
        device_id: u32,
        selector: u32,
        element: u32,
    ) -> Result<bool, AudioControlError> {
        let address = Self::build_output_property_address(selector, element);
        let mut is_settable: u8 = 0;
        let status = unsafe {
            AudioObjectIsPropertySettable(
                device_id,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
                NonNull::new(&raw mut is_settable).unwrap(),
            )
        };

        if status != 0 {
            return Err(AudioControlError::GetPropertyFailed(format!(
                "Failed to query property settable status (OSStatus: {status})"
            )));
        }

        Ok(is_settable != 0)
    }

    fn get_u32_property(
        device_id: u32,
        selector: u32,
        element: u32,
    ) -> Result<u32, AudioControlError> {
        let address = Self::build_output_property_address(selector, element);
        let mut value: u32 = 0;
        let mut size = u32::try_from(std::mem::size_of::<u32>()).map_err(|_| {
            AudioControlError::GetPropertyFailed(
                "Failed to convert u32 property size to CoreAudio size type".to_string(),
            )
        })?;

        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
                0,
                std::ptr::null(),
                NonNull::new(&raw mut size).unwrap(),
                NonNull::new((&raw mut value).cast::<c_void>()).unwrap(),
            )
        };

        if status != 0 {
            return Err(AudioControlError::GetPropertyFailed(format!(
                "OSStatus: {status}"
            )));
        }

        Ok(value)
    }

    fn set_u32_property(
        device_id: u32,
        selector: u32,
        element: u32,
        value: u32,
    ) -> Result<(), AudioControlError> {
        let address = Self::build_output_property_address(selector, element);
        let size = u32::try_from(std::mem::size_of::<u32>()).map_err(|_| {
            AudioControlError::SetPropertyFailed(
                "Failed to convert u32 property size to CoreAudio size type".to_string(),
            )
        })?;

        let status = unsafe {
            AudioObjectSetPropertyData(
                device_id,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
                0,
                std::ptr::null(),
                size,
                NonNull::new((&raw const value).cast_mut().cast::<c_void>()).unwrap(),
            )
        };

        if status != 0 {
            return Err(AudioControlError::SetPropertyFailed(format!(
                "OSStatus: {status}"
            )));
        }

        Ok(())
    }

    fn get_f32_property(
        device_id: u32,
        selector: u32,
        element: u32,
    ) -> Result<f32, AudioControlError> {
        let address = Self::build_output_property_address(selector, element);
        let mut value: f32 = 0.0;
        let mut size = u32::try_from(std::mem::size_of::<f32>()).map_err(|_| {
            AudioControlError::GetPropertyFailed(
                "Failed to convert f32 property size to CoreAudio size type".to_string(),
            )
        })?;

        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
                0,
                std::ptr::null(),
                NonNull::new(&raw mut size).unwrap(),
                NonNull::new((&raw mut value).cast::<c_void>()).unwrap(),
            )
        };

        if status != 0 {
            return Err(AudioControlError::GetPropertyFailed(format!(
                "OSStatus: {status}"
            )));
        }

        Ok(value)
    }

    fn set_f32_property(
        device_id: u32,
        selector: u32,
        element: u32,
        value: f32,
    ) -> Result<(), AudioControlError> {
        let address = Self::build_output_property_address(selector, element);
        let size = u32::try_from(std::mem::size_of::<f32>()).map_err(|_| {
            AudioControlError::SetPropertyFailed(
                "Failed to convert f32 property size to CoreAudio size type".to_string(),
            )
        })?;

        let status = unsafe {
            AudioObjectSetPropertyData(
                device_id,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
                0,
                std::ptr::null(),
                size,
                NonNull::new((&raw const value).cast_mut().cast::<c_void>()).unwrap(),
            )
        };

        if status != 0 {
            return Err(AudioControlError::SetPropertyFailed(format!(
                "OSStatus: {status}"
            )));
        }

        Ok(())
    }

    fn get_preferred_stereo_channels(device_id: u32) -> Result<[u32; 2], AudioControlError> {
        let address = Self::build_output_property_address(
            kAudioDevicePropertyPreferredChannelsForStereo,
            kAudioObjectPropertyElementMain,
        );
        let mut channels = [0u32; 2];
        let mut size = u32::try_from(std::mem::size_of_val(&channels)).map_err(|_| {
            AudioControlError::GetPropertyFailed(
                "Failed to convert stereo channel payload size to CoreAudio size type".to_string(),
            )
        })?;

        let status = unsafe {
            AudioObjectGetPropertyData(
                device_id,
                NonNull::new((&raw const address).cast_mut()).unwrap(),
                0,
                std::ptr::null(),
                NonNull::new(&raw mut size).unwrap(),
                NonNull::new((&raw mut channels).cast::<c_void>()).unwrap(),
            )
        };

        if status != 0 {
            return Err(AudioControlError::GetPropertyFailed(format!(
                "Failed to get preferred stereo channels (OSStatus: {status})"
            )));
        }

        Ok(channels)
    }

    fn select_mute_strategy_for_device(device_id: u32) -> MuteStrategy {
        let mute_property_exists = Self::has_property(
            device_id,
            kAudioDevicePropertyMute,
            kAudioObjectPropertyElementMain,
        );
        let mute_property_is_settable = mute_property_exists
            && Self::is_property_settable(
                device_id,
                kAudioDevicePropertyMute,
                kAudioObjectPropertyElementMain,
            )
            .unwrap_or(false);

        select_mute_strategy(mute_property_exists, mute_property_is_settable)
    }

    fn build_volume_zero_fallback_targets(device_id: u32) -> Vec<(u32, String)> {
        let mut volume_zero_fallback_targets =
            vec![(kAudioObjectPropertyElementMain, "main output".to_string())];

        match Self::get_preferred_stereo_channels(device_id) {
            Ok(preferred_stereo_channels) => {
                for (stereo_channel_index, stereo_channel_element) in
                    preferred_stereo_channels.into_iter().enumerate()
                {
                    if volume_zero_fallback_targets
                        .iter()
                        .any(|(existing_element, _)| *existing_element == stereo_channel_element)
                    {
                        continue;
                    }

                    let stereo_channel_label = if stereo_channel_index == 0 {
                        "left stereo channel"
                    } else {
                        "right stereo channel"
                    };
                    volume_zero_fallback_targets
                        .push((stereo_channel_element, stereo_channel_label.to_string()));
                }
            }
            Err(error) => {
                log::warn!(
                    "Failed to get preferred stereo channels while applying volume-zero fallback on device {device_id}: {error}"
                );
            }
        }

        volume_zero_fallback_targets
    }

    fn apply_volume_zero_fallback_and_capture_snapshot(
        device_id: u32,
    ) -> Result<Vec<OutputVolumeScalarSnapshot>, AudioControlError> {
        let volume_zero_fallback_targets = Self::build_volume_zero_fallback_targets(device_id);
        let mut latest_volume_zero_error: Option<AudioControlError> = None;
        let mut captured_volume_scalars: Vec<OutputVolumeScalarSnapshot> = Vec::new();

        for (volume_element, volume_element_label) in volume_zero_fallback_targets {
            match Self::is_property_settable(
                device_id,
                kAudioDevicePropertyVolumeScalar,
                volume_element,
            ) {
                Ok(true) => {}
                Ok(false) => {
                    continue;
                }
                Err(error) => {
                    log::warn!(
                        "Failed to query whether {volume_element_label} volume is settable on device {device_id}: {error}"
                    );
                    latest_volume_zero_error = Some(error);
                    continue;
                }
            }

            let initial_volume_scalar = match Self::get_f32_property(
                device_id,
                kAudioDevicePropertyVolumeScalar,
                volume_element,
            ) {
                Ok(initial_volume_scalar) => initial_volume_scalar,
                Err(error) => {
                    log::warn!(
                        "Failed to capture {volume_element_label} volume scalar on device {device_id}: {error}"
                    );
                    latest_volume_zero_error = Some(error);
                    continue;
                }
            };

            match Self::set_f32_property(
                device_id,
                kAudioDevicePropertyVolumeScalar,
                volume_element,
                0.0,
            ) {
                Ok(()) => {
                    captured_volume_scalars.push(OutputVolumeScalarSnapshot {
                        property_element: volume_element,
                        initial_volume_scalar,
                    });
                }
                Err(error) => {
                    log::warn!(
                        "Failed to set {volume_element_label} volume to zero on device {device_id}: {error}"
                    );
                    latest_volume_zero_error = Some(error);
                }
            }
        }

        if !captured_volume_scalars.is_empty() {
            return Ok(captured_volume_scalars);
        }

        Err(latest_volume_zero_error.unwrap_or_else(|| {
            AudioControlError::SetPropertyFailed(
                "Failed to apply volume-zero fallback: no settable volume controls were available"
                    .to_string(),
            )
        }))
    }

    fn restore_device_mute_session(
        output_device_id: u32,
        initial_device_muted: bool,
    ) -> Result<(), AudioControlError> {
        Self::set_u32_property(
            output_device_id,
            kAudioDevicePropertyMute,
            kAudioObjectPropertyElementMain,
            u32::from(initial_device_muted),
        )
    }

    fn restore_volume_zero_fallback_session(
        output_device_id: u32,
        captured_volume_scalars: &[OutputVolumeScalarSnapshot],
    ) -> Result<(), AudioControlError> {
        let mut latest_restore_error: Option<AudioControlError> = None;
        let mut successfully_restored_any_volume = false;

        for captured_volume_scalar in captured_volume_scalars {
            match Self::set_f32_property(
                output_device_id,
                kAudioDevicePropertyVolumeScalar,
                captured_volume_scalar.property_element,
                captured_volume_scalar.initial_volume_scalar,
            ) {
                Ok(()) => {
                    successfully_restored_any_volume = true;
                }
                Err(error) => {
                    log::warn!(
                        "Failed to restore volume scalar for element {}: {error}",
                        captured_volume_scalar.property_element
                    );
                    latest_restore_error = Some(error);
                }
            }
        }

        if successfully_restored_any_volume {
            return Ok(());
        }

        Err(latest_restore_error.unwrap_or_else(|| {
            AudioControlError::SetPropertyFailed(
                "Failed to restore macOS fallback volume: no captured controls were restorable"
                    .to_string(),
            )
        }))
    }
}

impl SystemAudioControl for MacOSAudioController {
    fn is_muted(&self) -> Result<bool, AudioControlError> {
        let current_default_output_device_id = Self::get_default_output_device()?;
        Self::get_u32_property(
            current_default_output_device_id,
            kAudioDevicePropertyMute,
            kAudioObjectPropertyElementMain,
        )
        .map(|raw_muted_value| raw_muted_value != 0)
    }

    fn begin_mute_session(&self) -> Result<ActiveMuteSession, AudioControlError> {
        let current_default_output_device_id = Self::get_default_output_device()?;
        let mute_strategy_to_apply =
            Self::select_mute_strategy_for_device(current_default_output_device_id);

        match mute_strategy_to_apply {
            MuteStrategy::DeviceMuteProperty => {
                log::info!("Applying mute strategy: DeviceMuteProperty");
                let initial_device_muted = Self::get_u32_property(
                    current_default_output_device_id,
                    kAudioDevicePropertyMute,
                    kAudioObjectPropertyElementMain,
                )? != 0;

                Self::set_u32_property(
                    current_default_output_device_id,
                    kAudioDevicePropertyMute,
                    kAudioObjectPropertyElementMain,
                    1,
                )?;

                Ok(ActiveMuteSession::MacOsDeviceMute {
                    output_device_id: current_default_output_device_id,
                    initial_device_muted,
                })
            }
            MuteStrategy::VolumeZeroFallback => {
                log::info!(
                    "Applying mute strategy: VolumeZeroFallback (mute property unavailable or unsettable)"
                );
                let captured_volume_scalars =
                    Self::apply_volume_zero_fallback_and_capture_snapshot(
                        current_default_output_device_id,
                    )?;

                Ok(ActiveMuteSession::MacOsVolumeZeroFallback {
                    output_device_id: current_default_output_device_id,
                    captured_volume_scalars,
                })
            }
        }
    }

    fn end_mute_session(
        &self,
        active_mute_session: &ActiveMuteSession,
    ) -> Result<(), AudioControlError> {
        match active_mute_session {
            ActiveMuteSession::MacOsDeviceMute {
                output_device_id,
                initial_device_muted,
            } => {
                Self::restore_device_mute_session(*output_device_id, *initial_device_muted)?;
                log::info!(
                    "Restored macOS output state after unmute using strategy DeviceMuteProperty"
                );
                Ok(())
            }
            ActiveMuteSession::MacOsVolumeZeroFallback {
                output_device_id,
                captured_volume_scalars,
            } => {
                Self::restore_volume_zero_fallback_session(
                    *output_device_id,
                    captured_volume_scalars,
                )?;
                log::info!(
                    "Restored macOS output state after unmute using strategy VolumeZeroFallback"
                );
                Ok(())
            }
        }
    }
}

fn select_mute_strategy(
    mute_property_exists: bool,
    mute_property_is_settable: bool,
) -> MuteStrategy {
    if mute_property_exists && mute_property_is_settable {
        MuteStrategy::DeviceMuteProperty
    } else {
        MuteStrategy::VolumeZeroFallback
    }
}

#[cfg(test)]
mod tests {
    use super::{select_mute_strategy, MuteStrategy};

    #[test]
    fn select_mute_strategy_prefers_device_mute_property_when_supported_and_settable() {
        assert_eq!(
            select_mute_strategy(true, true),
            MuteStrategy::DeviceMuteProperty
        );
    }

    #[test]
    fn select_mute_strategy_falls_back_to_volume_zero_when_mute_property_is_not_settable() {
        assert_eq!(
            select_mute_strategy(true, false),
            MuteStrategy::VolumeZeroFallback
        );
        assert_eq!(
            select_mute_strategy(false, false),
            MuteStrategy::VolumeZeroFallback
        );
    }
}
