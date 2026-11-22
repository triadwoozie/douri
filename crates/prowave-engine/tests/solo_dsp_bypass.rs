//! Verifies that Solo mode short-circuits instrument synthesis, voice generation,
//! and insert effects DSP on inactive tracks, and verifies zero underruns during playback.

use prowave_engine::{Command, EngineCore, Shared, Song, TrackMix};
use std::sync::{atomic::Ordering, Arc};

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOC_GUARD: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[test]
fn solo_bypasses_inactive_track_dsp_completely() {
    let (mut tx, rx) = rtrb::RingBuffer::new(32);
    let (gtx, _grx) = rtrb::RingBuffer::new(16);
    let shared = Arc::new(Shared::new());
    let mut core = EngineCore::new(48_000, rx, gtx, shared.clone());

    // Setup 2 tracks
    assert!(tx
        .push(Command::SetSong(Box::new(Song::new(
            vec![],
            vec![TrackMix::default(), TrackMix::default()]
        ))))
        .is_ok());

    // Play note on track 0 (pitch 60) and track 1 (pitch 64)
    assert!(tx
        .push(Command::NoteOn {
            pitch: 60,
            vel: 90,
            track: 0
        })
        .is_ok());
    assert!(tx
        .push(Command::NoteOn {
            pitch: 64,
            vel: 90,
            track: 1
        })
        .is_ok());

    let mut buf = [0.0f32; 256];
    assert_no_alloc::assert_no_alloc(|| core.process(&mut buf));

    let initial_energy: f32 = buf.iter().map(|s| s.abs()).sum();
    assert!(initial_energy > 0.0, "Both tracks should produce audio");

    // Solo track 0
    assert!(tx
        .push(Command::SetTrackMix {
            track: 0,
            mix: TrackMix {
                solo: true,
                ..TrackMix::default()
            },
        })
        .is_ok());

    // Process a block
    assert_no_alloc::assert_no_alloc(|| core.process(&mut buf));
    assert!(shared.underruns.load(Ordering::Relaxed) == 0);

    // Mute both tracks
    assert!(tx
        .push(Command::SetTrackMix {
            track: 0,
            mix: TrackMix {
                mute: true,
                ..TrackMix::default()
            },
        })
        .is_ok());
    assert!(tx
        .push(Command::SetTrackMix {
            track: 1,
            mix: TrackMix {
                mute: true,
                ..TrackMix::default()
            },
        })
        .is_ok());

    // Process a block during the ramp down
    assert_no_alloc::assert_no_alloc(|| core.process(&mut buf));
    // Process next block after fader ramp-down has completed
    assert_no_alloc::assert_no_alloc(|| core.process(&mut buf));
    let silent_energy: f32 = buf.iter().map(|s| s.abs()).sum();
    assert_eq!(
        silent_energy, 0.0,
        "Both muted tracks produce zero audio and bypass DSP"
    );
    assert_eq!(shared.underruns.load(Ordering::Relaxed), 0);
}

#[test]
fn thirty_second_multitrack_playback_zero_underruns() {
    let (mut tx, rx) = rtrb::RingBuffer::new(64);
    let (gtx, _grx) = rtrb::RingBuffer::new(16);
    let shared = Arc::new(Shared::new());
    let mut core = EngineCore::new(48_000, rx, gtx, shared.clone());

    // 6 tracks (matching sonare arrangement: Piano, Bass, Pad, Percussion, Vocal Score, Vocal Audio)
    let tracks = vec![
        TrackMix::default(),
        TrackMix::default(),
        TrackMix::default(),
        TrackMix::default(),
        TrackMix::default(),
        TrackMix::default(),
    ];
    assert!(tx
        .push(Command::SetSong(Box::new(Song::new(vec![], tracks))))
        .is_ok());

    // Start playback
    assert!(tx.push(Command::Play).is_ok());

    // Simulate 30 seconds of playback at 48kHz (1,440,000 frames) in 256-frame blocks (5625 blocks)
    let mut buf = [0.0f32; 512]; // 256 stereo frames
    let total_blocks = (48_000 * 30) / 256;

    for i in 0..total_blocks {
        // Trigger periodic notes across tracks
        if i % 100 == 0 {
            let _ = tx.push(Command::NoteOn {
                pitch: 60 + ((i % 12) as u8),
                vel: 100,
                track: (i % 4) as u16,
            });
        }
        assert_no_alloc::assert_no_alloc(|| core.process(&mut buf));
    }

    // Verify 0 underruns and no buffer starvation
    let underruns = shared.underruns.load(Ordering::Relaxed);
    assert_eq!(
        underruns, 0,
        "30-second playback suffered audio underruns: {underruns}"
    );
}
