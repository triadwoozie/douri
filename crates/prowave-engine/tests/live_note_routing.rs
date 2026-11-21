//! Logic-style instrument-track audition: same-pitch notes release independently.
use prowave_engine::{Command, EngineCore, Shared, Song, TrackMix};
use std::sync::Arc;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOC_GUARD: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

#[test]
fn independent_live_note_release_does_not_allocate() {
    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, _grx) = rtrb::RingBuffer::new(8);
    let mut core = EngineCore::new(48_000, rx, gtx, Arc::new(Shared::new()));
    let tracks = vec![
        TrackMix {
            pan: -1.0,
            ..TrackMix::default()
        },
        TrackMix {
            pan: 1.0,
            ..TrackMix::default()
        },
    ];
    assert!(tx
        .push(Command::SetSong(Box::new(Song::new(vec![], tracks))))
        .is_ok());
    for track in 0..2 {
        assert!(tx
            .push(Command::NoteOn {
                pitch: 69,
                vel: 100,
                track
            })
            .is_ok());
    }
    let mut audio = vec![0.0; 48_000]; // half a second, stereo
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio));
    for channel in 0..2 {
        assert!(audio
            .iter()
            .skip(channel)
            .step_by(2)
            .any(|x| x.abs() > 0.01));
    }
    assert!(tx
        .push(Command::NoteOffTrack {
            pitch: 69,
            track: 0
        })
        .is_ok());
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio)); // release tail
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio));
    assert!(audio.iter().all(|x| x.is_finite()));
    assert!(audio.iter().step_by(2).all(|x| x.abs() < 1e-6));
    assert!(audio.iter().skip(1).step_by(2).any(|x| x.abs() > 0.01));
    assert!(tx
        .push(Command::NoteOffTrack {
            pitch: 69,
            track: 1
        })
        .is_ok());
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio));
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio));
    assert!(audio.iter().all(|x| x.abs() < 1e-6));
}

#[test]
fn lock_free_live_midi_ring_and_armed_track_routing_zero_alloc() {
    use prowave_engine::LiveMidiEvent;
    use std::sync::atomic::{AtomicUsize, Ordering};

    let (mut tx, rx) = rtrb::RingBuffer::new(16);
    let (gtx, _grx) = rtrb::RingBuffer::new(8);
    let (mut midi_tx, midi_rx) = rtrb::RingBuffer::<LiveMidiEvent>::new(64);
    let armed = Arc::new(AtomicUsize::new(0));

    let mut core = EngineCore::new(48_000, rx, gtx, Arc::new(Shared::new()));
    core.set_live_midi(midi_rx, armed.clone());

    let tracks = vec![
        TrackMix {
            pan: -1.0,
            ..TrackMix::default()
        },
        TrackMix {
            pan: 1.0,
            ..TrackMix::default()
        },
    ];
    assert!(tx
        .push(Command::SetSong(Box::new(Song::new(vec![], tracks))))
        .is_ok());

    // Play note on armed track 0 via live midi ring buffer
    assert!(midi_tx
        .push(LiveMidiEvent::NoteOn {
            pitch: 60,
            vel: 90,
            track: None
        })
        .is_ok());

    let mut audio = vec![0.0; 4800]; // 50ms block
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio));

    // Track 0 is panned left (channel 0)
    assert!(audio.iter().step_by(2).any(|x| x.abs() > 0.01));
    assert!(audio.iter().skip(1).step_by(2).all(|x| x.abs() < 1e-4));

    // Switch armed track to 1 and send AllNotesOff for track 0
    armed.store(1, Ordering::Relaxed);
    assert!(tx.push(Command::AllNotesOff { track: 0 }).is_ok());

    // Send note to newly armed track 1 via live midi ring buffer
    assert!(midi_tx
        .push(LiveMidiEvent::NoteOn {
            pitch: 72,
            vel: 90,
            track: None
        })
        .is_ok());

    // Process: note on track 0 stops, track 1 sounds panned right (channel 1)
    let mut audio2 = vec![0.0; 4800];
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio2));
    assert_no_alloc::assert_no_alloc(|| core.process(&mut audio2));

    assert!(audio2.iter().skip(1).step_by(2).any(|x| x.abs() > 0.01));
}
