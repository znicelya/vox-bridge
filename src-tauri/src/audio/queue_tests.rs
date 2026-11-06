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

#[tokio::test]
async fn close_drains_accepted_frames_rejects_new_frames_and_is_replayable() {
    let queue = BoundedAudioQueue::new(2);
    for sequence in 0..3 {
        queue.push_sync(fixture_frame(sequence)).unwrap();
    }
    queue.close();
    queue.close();
    assert_eq!(queue.push_sync(fixture_frame(3)).unwrap_err().sequence(), 3);
    assert_eq!(queue.recv().await.unwrap().sequence(), 1);
    assert_eq!(queue.recv().await.unwrap().sequence(), 2);
    assert!(queue.recv().await.is_none());
    assert!(queue.recv().await.is_none());
    assert_eq!(queue.dropped_frames(), 1);
    assert_eq!(queue.clear(), 0);
    assert!(queue.push(fixture_frame(4)).await.is_err());
}

#[tokio::test]
async fn clear_discards_only_pending_frames_without_reopening_or_counting_overflow() {
    let queue = BoundedAudioQueue::new(2);
    queue.push_sync(fixture_frame(1)).unwrap();
    queue.push_sync(fixture_frame(2)).unwrap();
    assert_eq!(queue.clear(), 2);
    assert_eq!(queue.dropped_frames(), 0);
    queue.push_sync(fixture_frame(3)).unwrap();
    queue.close();
    assert_eq!(queue.clear(), 1);
    assert!(queue.recv().await.is_none());
    assert!(queue.push_sync(fixture_frame(4)).is_err());
}

#[tokio::test]
async fn waiting_receivers_receive_push_and_all_wake_when_closed() {
    let queue = BoundedAudioQueue::new(2);
    let receiver = {
        let queue = queue.clone();
        tokio::spawn(async move { queue.recv().await })
    };
    tokio::task::yield_now().await;
    queue.push_sync(fixture_frame(9)).unwrap();
    assert_eq!(
        tokio::time::timeout(std::time::Duration::from_secs(1), receiver)
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .sequence(),
        9
    );
    let mut waiters = Vec::new();
    for _ in 0..8 {
        let queue = queue.clone();
        waiters.push(tokio::spawn(async move { queue.recv().await }));
    }
    tokio::task::yield_now().await;
    queue.close();
    for waiter in waiters {
        assert!(
            tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
                .await
                .unwrap()
                .unwrap()
                .is_none()
        );
    }
}
