use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
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
            Some(playback_started_rx),
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

#[cfg(desktop)]
#[test]
fn auto_mute_proceeds_without_sound_when_commit_fires() {
    let (wait_started_tx, wait_started_rx) = mpsc::channel();
    let (completion_tx, completion_rx) = mpsc::channel();
    let recording_start_committed = Arc::new(AtomicBool::new(false));
    let recording_start_committed_for_thread = Arc::clone(&recording_start_committed);

    let handle = thread::spawn(move || {
        let result = crate::wait_for_recording_start_commit(
            None,
            recording_start_committed_for_thread,
            Duration::from_millis(250),
            Duration::from_millis(500),
            Some(wait_started_tx),
        );
        completion_tx.send(result).unwrap();
    });

    wait_started_rx.recv().unwrap();
    assert!(
        completion_rx.try_recv().is_err(),
        "should not complete before commit"
    );

    recording_start_committed.store(true, Ordering::Release);

    assert!(
        completion_rx.recv().unwrap(),
        "should return true once commit fires"
    );
    handle.join().unwrap();
}

#[cfg(desktop)]
#[test]
fn auto_mute_does_not_proceed_without_sound_when_commit_never_fires() {
    let (completion_tx, completion_rx) = mpsc::channel();
    let recording_start_committed = Arc::new(AtomicBool::new(false));

    let handle = thread::spawn(move || {
        let result = crate::wait_for_recording_start_commit(
            None,
            recording_start_committed,
            Duration::from_millis(10),
            Duration::from_millis(50),
            None,
        );
        completion_tx.send(result).unwrap();
    });

    let result = completion_rx
        .recv_timeout(Duration::from_millis(500))
        .expect("should complete within timeout");
    assert!(!result, "should return false when commit never fires");
    handle.join().unwrap();
}
