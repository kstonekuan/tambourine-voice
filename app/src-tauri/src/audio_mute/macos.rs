//! macOS audio mute control implementation using `CoreAudio`.
//!
//! Uses the `CoreAudio` framework to control the default audio output device's
//! mute state via `AudioObject` property APIs.

use super::{AudioControlError, SystemAudioControl};
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
use std::sync::Mutex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MuteStrategy {
    DeviceMuteProperty,
    VolumeZeroFallback,
}

#[derive(Debug, Clone, Copy)]
struct MacOsOutputStateSnapshot {
    output_device_id: u32,
    initial_device_muted: bool,
    initial_main_volume_scalar: Option<f32>,
    initial_stereo_channel_volume_scalars: Option<[f32; 2]>,
    mute_strategy_used: MuteStrategy,
}

/// macOS audio controller using `CoreAudio`.
pub struct MacOSAudioController {
    output_state_snapshot: Mutex<Option<MacOsOutputStateSnapshot>>,
}

// SAFETY: CoreAudio APIs are thread-safe
unsafe impl Send for MacOSAudioController {}
unsafe impl Sync for MacOSAudioController {}

impl MacOSAudioController {
    /// Create a new macOS audio controller.
    pub fn new() -> Result<Self, AudioControlError> {
        let _ = Self::get_default_output_device()?;
        Ok(Self {
            output_state_snapshot: Mutex::new(None),
        })
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

    fn apply_volume_zero_fallback(device_id: u32) -> Result<(), AudioControlError> {
        let mut successfully_muted_any_volume_control = false;
        let mut latest_volume_zero_error: Option<AudioControlError> = None;

        let mut attempt_volume_zero_for_element =
            |volume_element: u32, volume_element_label: &str| match Self::is_property_settable(
                device_id,
                kAudioDevicePropertyVolumeScalar,
                volume_element,
            ) {
                Ok(true) => match Self::set_f32_property(
                    device_id,
                    kAudioDevicePropertyVolumeScalar,
                    volume_element,
                    0.0,
                ) {
                    Ok(()) => {
                        successfully_muted_any_volume_control = true;
                    }
                    Err(error) => {
                        log::warn!(
                            "Failed to set {volume_element_label} volume to zero on device {device_id}: {error}"
                        );
                        latest_volume_zero_error = Some(error);
                    }
                },
                Ok(false) => {}
                Err(error) => {
                    log::warn!(
                        "Failed to query whether {volume_element_label} volume is settable on device {device_id}: {error}"
                    );
                    latest_volume_zero_error = Some(error);
                }
            };

        attempt_volume_zero_for_element(kAudioObjectPropertyElementMain, "main output");

        match Self::get_preferred_stereo_channels(device_id) {
            Ok(preferred_stereo_channels) => {
                for (stereo_channel_index, stereo_channel_element) in
                    preferred_stereo_channels.into_iter().enumerate()
                {
                    let stereo_channel_label = if stereo_channel_index == 0 {
                        "left stereo channel"
                    } else {
                        "right stereo channel"
                    };
                    attempt_volume_zero_for_element(stereo_channel_element, stereo_channel_label);
                }
            }
            Err(error) => {
                log::warn!(
                    "Failed to get preferred stereo channels while applying volume-zero fallback on device {device_id}: {error}"
                );
                latest_volume_zero_error = Some(error);
            }
        }

        if successfully_muted_any_volume_control {
            return Ok(());
        }

        Err(latest_volume_zero_error.unwrap_or_else(|| {
            AudioControlError::SetPropertyFailed(
                "Failed to apply volume-zero fallback: no settable volume controls were available"
                    .to_string(),
            )
        }))
    }

    fn capture_output_state_snapshot(
        device_id: u32,
        mute_strategy_used: MuteStrategy,
    ) -> MacOsOutputStateSnapshot {
        let initial_device_muted = Self::get_u32_property(
            device_id,
            kAudioDevicePropertyMute,
            kAudioObjectPropertyElementMain,
        )
        .map(|raw_muted_value| raw_muted_value != 0)
        .unwrap_or(false);

        let initial_main_volume_scalar = Self::get_f32_property(
            device_id,
            kAudioDevicePropertyVolumeScalar,
            kAudioObjectPropertyElementMain,
        )
        .ok();

        let initial_stereo_channel_volume_scalars = Self::get_preferred_stereo_channels(device_id)
            .ok()
            .and_then(|preferred_stereo_channels| {
                let left_channel_volume = Self::get_f32_property(
                    device_id,
                    kAudioDevicePropertyVolumeScalar,
                    preferred_stereo_channels[0],
                )
                .ok()?;
                let right_channel_volume = Self::get_f32_property(
                    device_id,
                    kAudioDevicePropertyVolumeScalar,
                    preferred_stereo_channels[1],
                )
                .ok()?;
                Some([left_channel_volume, right_channel_volume])
            });

        MacOsOutputStateSnapshot {
            output_device_id: device_id,
            initial_device_muted,
            initial_main_volume_scalar,
            initial_stereo_channel_volume_scalars,
            mute_strategy_used,
        }
    }

    fn restore_snapshot_to_original_device(output_state_snapshot: MacOsOutputStateSnapshot) {
        let original_output_device_id = output_state_snapshot.output_device_id;
        if let Err(error) = Self::set_u32_property(
            original_output_device_id,
            kAudioDevicePropertyMute,
            kAudioObjectPropertyElementMain,
            u32::from(output_state_snapshot.initial_device_muted),
        ) {
            log::warn!("Failed to restore device mute state: {error}");
        }

        if let Some(initial_main_volume_scalar) = output_state_snapshot.initial_main_volume_scalar {
            if let Err(error) = Self::set_f32_property(
                original_output_device_id,
                kAudioDevicePropertyVolumeScalar,
                kAudioObjectPropertyElementMain,
                initial_main_volume_scalar,
            ) {
                log::warn!("Failed to restore main volume scalar: {error}");
            }
        }

        if let Some(initial_stereo_channel_volume_scalars) =
            output_state_snapshot.initial_stereo_channel_volume_scalars
        {
            match Self::get_preferred_stereo_channels(original_output_device_id) {
                Ok(preferred_stereo_channels) => {
                    if let Err(error) = Self::set_f32_property(
                        original_output_device_id,
                        kAudioDevicePropertyVolumeScalar,
                        preferred_stereo_channels[0],
                        initial_stereo_channel_volume_scalars[0],
                    ) {
                        log::warn!("Failed to restore left stereo channel volume: {error}");
                    }

                    if let Err(error) = Self::set_f32_property(
                        original_output_device_id,
                        kAudioDevicePropertyVolumeScalar,
                        preferred_stereo_channels[1],
                        initial_stereo_channel_volume_scalars[1],
                    ) {
                        log::warn!("Failed to restore right stereo channel volume: {error}");
                    }
                }
                Err(error) => {
                    log::warn!("Failed to get preferred stereo channels during restore: {error}");
                }
            }
        }
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

    fn set_muted(&self, muted: bool) -> Result<(), AudioControlError> {
        let current_default_output_device_id = Self::get_default_output_device()?;

        if muted {
            let mute_strategy_to_apply =
                Self::select_mute_strategy_for_device(current_default_output_device_id);
            let output_state_snapshot = Self::capture_output_state_snapshot(
                current_default_output_device_id,
                mute_strategy_to_apply,
            );

            match mute_strategy_to_apply {
                MuteStrategy::DeviceMuteProperty => {
                    log::info!("Applying mute strategy: DeviceMuteProperty");
                    Self::set_u32_property(
                        current_default_output_device_id,
                        kAudioDevicePropertyMute,
                        kAudioObjectPropertyElementMain,
                        1,
                    )?;
                }
                MuteStrategy::VolumeZeroFallback => {
                    log::info!(
                        "Applying mute strategy: VolumeZeroFallback (mute property unavailable or unsettable)"
                    );
                    Self::apply_volume_zero_fallback(current_default_output_device_id)?;
                }
            }

            *self.output_state_snapshot.lock().unwrap() = Some(output_state_snapshot);
            return Ok(());
        }

        let mut snapshot_guard = self.output_state_snapshot.lock().unwrap();
        if let Some(output_state_snapshot) = *snapshot_guard {
            Self::restore_snapshot_to_original_device(output_state_snapshot);
            snapshot_guard.take();
            log::info!(
                "Restored macOS output state after unmute using strategy {:?}",
                output_state_snapshot.mute_strategy_used
            );
            return Ok(());
        }

        log::info!("No macOS output state snapshot found, using best-effort unmute");
        if let Err(error) = Self::set_u32_property(
            current_default_output_device_id,
            kAudioDevicePropertyMute,
            kAudioObjectPropertyElementMain,
            0,
        ) {
            log::warn!("Best-effort unmute failed: {error}");
        }
        Ok(())
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
