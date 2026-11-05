use crate::{
    audio::{AudioFrame, PcmSpec, queue::BoundedAudioQueue},
    domain::audio::AudioSource,
};

fn fixture_frame(sequence: u64) -> AudioFrame {
    AudioFrame::new(
        AudioSource::Microphone,
        sequence,
        PcmSpec::pcm16_mono(16_000),
        vec![sequence as i16],
    )
    .unwrap()
}

#[tokio::test]
async fn full_audio_queue_drops_oldest_and_reports_backpressure() {
    let queue = BoundedAudioQueue::new(2);
    queue.push(fixture_frame(1)).await.unwrap();
    queue.push(fixture_frame(2)).await.unwrap();
    queue.push(fixture_frame(3)).await.unwrap();

    assert_eq!(queue.dropped_frames(), 1);
    assert_eq!(queue.pop().await.unwrap().sequence(), 2);
    assert_eq!(queue.pop().await.unwrap().sequence(), 3);
    assert!(queue.pop().await.is_none());
}

#[tokio::test]
async fn default_queue_is_bounded_to_fifty_frames() {
    let queue = BoundedAudioQueue::default();
    for sequence in 0..60 {
        queue.push(fixture_frame(sequence)).await.unwrap();
    }

    assert_eq!(queue.capacity_frames(), 50);
    assert_eq!(queue.dropped_frames(), 10);
    assert_eq!(queue.pop().await.unwrap().sequence(), 10);
}

#[tokio::test]
async fn zero_capacity_is_normalized_to_one_frame() {
    let queue = BoundedAudioQueue::new(0);
    queue.push(fixture_frame(1)).await.unwrap();
    queue.push(fixture_frame(2)).await.unwrap();

    assert_eq!(queue.capacity_frames(), 1);
    assert_eq!(queue.dropped_frames(), 1);
    assert_eq!(queue.pop().await.unwrap().sequence(), 2);
}
