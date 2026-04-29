use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc,
    Arc,
};
use std::thread;
use std::time::Duration;

#[cfg(desktop)]
#[test]
fn auto_mute_waits_for_start_sound_and_commit_before_proceeding() {
    let (wait_started_tx, wait_started_rx) = mpsc::channel();
    let (playback_started_tx, playback_started_rx) = mpsc::channel();
    let (completion_tx, completion_rx) = mpsc::channel();
    let recording_start_committed = Arc::new(AtomicBool::new(false));
    let recording_start_committed_for_thread = Arc::clone(&recording_start_committed);

    let handle = thread::spawn(move || {
        let result = crate::wait_for_recording_start_commit(
            playback_started_rx,
            recording_start_committed_for_thread,
            Duration::from_millis(250),
            Duration::from_millis(500),
            Some(wait_started_tx),
        );
        completion_tx.send(result).unwrap();
    });

    wait_started_rx.recv().unwrap();
    assert!(completion_rx.try_recv().is_err());

    recording_start_committed.store(true, Ordering::Release);
    playback_started_tx.send(()).unwrap();

    assert!(completion_rx.recv().unwrap());
    handle.join().unwrap();
}
