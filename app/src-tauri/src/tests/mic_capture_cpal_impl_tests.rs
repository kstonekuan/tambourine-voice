use super::{
    convert_i16_sample_to_normalized_f32, convert_u16_sample_to_normalized_f32,
    emit_normalized_audio_data_in_chunks, normalize_interleaved_input_chunk, AudioStreamNormalizer,
    MAX_NATIVE_AUDIO_EVENT_SAMPLES,
};
use std::sync::{Arc, Mutex};

fn assert_samples_are_close(actual: &[f32], expected: &[f32], tolerance: f32) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "sample lengths differ: actual={}, expected={}",
        actual.len(),
        expected.len()
    );
    for (index, (actual_sample, expected_sample)) in actual.iter().zip(expected.iter()).enumerate()
    {
        let delta = (actual_sample - expected_sample).abs();
        assert!(
            delta <= tolerance,
            "sample mismatch at index {index}: actual={actual_sample}, expected={expected_sample}, delta={delta}"
        );
    }
}

#[test]
fn normalize_interleaved_input_chunk_keeps_48khz_mono_data_stable() {
    let mut normalizer = AudioStreamNormalizer::new(1, 48_000);

    let first_chunk =
        normalize_interleaved_input_chunk(&[0.0_f32, 0.5_f32], &mut normalizer, |sample| sample);
    let second_chunk =
        normalize_interleaved_input_chunk(&[-0.25_f32, 1.0_f32], &mut normalizer, |sample| sample);

    let mut combined_output = first_chunk;
    combined_output.extend(second_chunk);

    assert_samples_are_close(&combined_output, &[0.0, 0.5, -0.25, 1.0], 1e-6);
}

#[test]
fn normalize_interleaved_input_chunk_downmixes_stereo_to_mono() {
    let mut normalizer = AudioStreamNormalizer::new(2, 48_000);
    let normalized_output = normalize_interleaved_input_chunk(
        &[1.0_f32, -1.0_f32, 0.25_f32, 0.75_f32],
        &mut normalizer,
        |sample| sample,
    );

    assert_samples_are_close(&normalized_output, &[0.0, 0.5], 1e-6);
}

#[test]
fn normalize_interleaved_input_chunk_upsamples_16khz_input_to_48khz() {
    let mut normalizer = AudioStreamNormalizer::new(1, 16_000);
    let normalized_output =
        normalize_interleaved_input_chunk(&[0.0_f32, 1.0_f32], &mut normalizer, |sample| sample);

    assert_samples_are_close(&normalized_output, &[0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0], 1e-5);
}

#[test]
fn emit_normalized_audio_data_in_chunks_splits_oversized_payloads() {
    let emitted_payloads: Arc<Mutex<Vec<Vec<f32>>>> = Arc::new(Mutex::new(Vec::new()));
    let emitted_payloads_for_callback = Arc::clone(&emitted_payloads);

    let on_audio_data: Arc<dyn Fn(Vec<f32>) + Send + Sync> = Arc::new(move |audio_data| {
        emitted_payloads_for_callback
            .lock()
            .unwrap()
            .push(audio_data);
    });

    let oversized_sample_count = MAX_NATIVE_AUDIO_EVENT_SAMPLES + 5;
    let oversized_audio_data = vec![0.25_f32; oversized_sample_count];
    emit_normalized_audio_data_in_chunks(oversized_audio_data, &on_audio_data);

    let emitted_payloads = emitted_payloads.lock().unwrap();
    assert_eq!(emitted_payloads.len(), 2);
    assert_eq!(emitted_payloads[0].len(), MAX_NATIVE_AUDIO_EVENT_SAMPLES);
    assert_eq!(emitted_payloads[1].len(), 5);
    assert!(emitted_payloads[0]
        .iter()
        .all(|sample| (*sample - 0.25).abs() <= f32::EPSILON));
    assert!(emitted_payloads[1]
        .iter()
        .all(|sample| (*sample - 0.25).abs() <= f32::EPSILON));
}

#[test]
fn convert_i16_sample_to_normalized_f32_stays_in_expected_range() {
    let converted_samples = [
        convert_i16_sample_to_normalized_f32(i16::MIN),
        convert_i16_sample_to_normalized_f32(0),
        convert_i16_sample_to_normalized_f32(i16::MAX),
    ];
    assert_samples_are_close(&converted_samples, &[-1.0, 0.0, 1.0], 0.0);
}

#[test]
fn convert_u16_sample_to_normalized_f32_maps_midpoint_to_zero() {
    let converted_samples = [
        convert_u16_sample_to_normalized_f32(u16::MIN),
        convert_u16_sample_to_normalized_f32(32_768),
        convert_u16_sample_to_normalized_f32(u16::MAX),
    ];
    let expected_maximum = 32_767.0 / 32_768.0;
    assert_samples_are_close(&converted_samples, &[-1.0, 0.0, expected_maximum], 0.0);
}
