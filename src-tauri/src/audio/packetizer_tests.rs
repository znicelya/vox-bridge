use std::time::{Duration, SystemTime};

use super::{AudioFrame, PcmSpec, packetizer::AudioPacketizer};
use crate::domain::audio::AudioSource;

fn frame(sequence: u64, at: SystemTime, samples: Vec<i16>) -> AudioFrame {
    AudioFrame::at(
        AudioSource::System,
        sequence,
        at,
        PcmSpec::pcm16_mono(16_000),
        samples,
    )
    .unwrap()
}

#[test]
fn fragments_preserve_samples_and_time_then_flush_once() {
    let at = SystemTime::UNIX_EPOCH + Duration::from_secs(7);
    let mut p = AudioPacketizer::new(AudioSource::System);
    let mut packets = Vec::new();
    for (sequence, (offset, count)) in [(0, 700), (700, 1800)].into_iter().enumerate() {
        packets.extend(
            p.push(frame(
                sequence as u64,
                at + Duration::from_micros(offset as u64 * 1_000_000 / 16_000),
                (offset..offset + count).map(|n| n as i16).collect(),
            ))
            .unwrap(),
        );
    }
    assert_eq!(packets.len(), 1);
    assert_eq!(packets[0].samples().len(), 1280);
    packets.push(p.flush().unwrap().unwrap());
    assert!(p.flush().unwrap().is_none());
    assert_eq!(packets[1].captured_at(), at + Duration::from_millis(80));
    assert_eq!(
        packets.iter().map(AudioFrame::sequence).collect::<Vec<_>>(),
        [0, 1]
    );
    let all: Vec<i16> = packets
        .iter()
        .flat_map(|p| p.samples().iter().copied())
        .collect();
    assert_eq!(all, (0..2500).map(|n| n as i16).collect::<Vec<_>>());
}

#[test]
fn gaps_discard_carry_and_reset_accepts_a_new_input_sequence() {
    let at = SystemTime::UNIX_EPOCH;
    let mut p = AudioPacketizer::new(AudioSource::System);
    p.push(frame(4, at, vec![1; 1400])).unwrap();
    let later = at + Duration::from_secs(2);
    let packet = p.push(frame(6, later, vec![2; 1280])).unwrap().remove(0);
    assert_eq!(packet.samples(), vec![2; 1280]);
    assert_eq!(packet.captured_at(), later);
    assert_eq!(packet.sequence(), 1);
    p.push(frame(7, later, vec![3; 100])).unwrap();
    p.reset();
    let packet = p.push(frame(0, later, vec![4; 1280])).unwrap().remove(0);
    assert_eq!(packet.samples(), vec![4; 1280]);
    assert_eq!(packet.sequence(), 2);
    assert!(p.flush().unwrap().is_none());
}

#[test]
fn invalid_or_stale_input_leaves_the_pending_audio_unchanged() {
    let at = SystemTime::UNIX_EPOCH;
    let mut p = AudioPacketizer::new(AudioSource::System);
    p.push(frame(5, at, vec![1; 100])).unwrap();
    for sequence in [4, 5] {
        assert!(p.push(frame(sequence, at, vec![9; 1280])).is_err());
    }
    let wrong_source = AudioFrame::at(
        AudioSource::Microphone,
        6,
        at,
        PcmSpec::pcm16_mono(16_000),
        vec![9; 1280],
    )
    .unwrap();
    let wrong_spec = AudioFrame::at(
        AudioSource::System,
        6,
        at,
        PcmSpec::pcm16_mono(24_000),
        vec![9; 1280],
    )
    .unwrap();
    for invalid in [wrong_source, wrong_spec, frame(6, at, vec![9; 16_001])] {
        assert!(p.push(invalid).is_err());
    }
    let packet = p.push(frame(6, at, vec![2; 1180])).unwrap().remove(0);
    assert_eq!(&packet.samples()[..100], vec![1; 100]);
    assert_eq!(&packet.samples()[100..], vec![2; 1180]);
    assert_eq!(packet.sequence(), 0);
}

#[test]
fn maximum_input_emits_bounded_packets_and_one_tail() {
    let mut p = AudioPacketizer::new(AudioSource::System);
    let mut packets = p
        .push(frame(0, SystemTime::UNIX_EPOCH, vec![7; 16_000]))
        .unwrap();
    assert_eq!(packets.len(), 12);
    assert!(packets.iter().all(|p| p.samples().len() == 1280));
    packets.push(p.flush().unwrap().unwrap());
    assert_eq!(packets.last().unwrap().samples().len(), 640);
    assert_eq!(
        packets.iter().map(|p| p.samples().len()).sum::<usize>(),
        16_000
    );
}
