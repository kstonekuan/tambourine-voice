use rodio::source::Source;
use rodio::{Decoder, DeviceSinkBuilder};
use std::io::Cursor;
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

/// Types of sounds that can be played
#[derive(Debug, Clone, Copy)]
pub enum SoundType {
    Start,
    Stop,
    Unavailable,
}

const START_SOUND: &[u8] = include_bytes!("assets/start.mp3");
const STOP_SOUND: &[u8] = include_bytes!("assets/stop.mp3");
const RECORDING_UNAVAILABLE_SOUND: &[u8] = include_bytes!("assets/recording-unavailable.mp3");
const DEFAULT_AUDIO_PLAYBACK_DURATION_MS: u64 = 500;

/// Play a sound effect (non-blocking)
pub fn play_sound(sound_type: SoundType) {
    play_sound_with_notify(sound_type, None);
}

/// Play a sound effect (non-blocking), optionally sending a notification once
/// playback has been submitted to the sink.
pub fn play_sound_with_notify(sound_type: SoundType, notify: Option<Sender<()>>) {
    thread::spawn(move || {
        if let Err(e) = play_sound_blocking(sound_type, notify) {
            log::warn!("Failed to play sound: {e}");
        }
    });
}

fn play_sound_blocking(
    sound_type: SoundType,
    notify: Option<Sender<()>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut sink = DeviceSinkBuilder::open_default_sink()?;
    sink.log_on_drop(false);

    let sound_data = match sound_type {
        SoundType::Start => START_SOUND,
        SoundType::Stop => STOP_SOUND,
        SoundType::Unavailable => RECORDING_UNAVAILABLE_SOUND,
    };

    let cursor = Cursor::new(sound_data);
    let source = Decoder::new(cursor)?.amplify(0.3);

    let duration = source
        .total_duration()
        .unwrap_or(Duration::from_millis(DEFAULT_AUDIO_PLAYBACK_DURATION_MS));

    sink.mixer().add(source);
    if let Some(tx) = notify {
        let _ = tx.send(());
    }
    thread::sleep(duration + Duration::from_millis(50));

    Ok(())
}
