//! The Singer instrument in the engine: its phrases play at their song seconds through the
//! tempo map (tempo changes included), a block of plug-in delay late like any instrument's, and
//! a new score swaps in while the engine plays, all without allocating or freeing on the audio
//! thread.
use prowave_engine::{
    Command, EngineCore, Garbage, PluginTrack, Shared, SingerClip, SingerPlayer, SingerScore, Song,
    TempoCurve, TempoEvent, TempoMap, TrackMix,
};
use std::sync::Arc;

#[cfg(debug_assertions)]
#[global_allocator]
static ALLOC_GUARD: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

const SR: u32 = 48_000;
const BLOCK: usize = 256;

/// 120 BPM, then 60 BPM from beat 4 (2 s): beat 6 is at 4 s.
fn tempo() -> TempoMap {
    let at = |beat, bpm| TempoEvent {
        beat,
        bpm,
        curve: TempoCurve::Step,
    };
    TempoMap::new(vec![at(0.0, 120.0), at(4.0, 60.0)], Vec::new())
}

fn first_sound(out: &[f32]) -> Option<usize> {
    out.chunks(2).position(|f| f[0].abs() > 1e-6)
}

#[test]
fn phrases_play_at_their_song_seconds_without_allocating() {
    let (mut tx, rx) = rtrb::RingBuffer::new(64);
    let (gtx, mut grx) = rtrb::RingBuffer::<Garbage>::new(64);
    let mut core = EngineCore::new(SR, rx, gtx, Arc::new(Shared::new()));
    let map = tempo();
    let (player, mut remote) = SingerPlayer::new(SR, BLOCK);
    let clip = |origin: f64| SingerClip {
        origin,
        sound: vec![0.5f32; SR as usize / 10].into(),
    };
    // One phrase at 1 s and one at 4.5 s (after the tempo change).
    let score = SingerScore::new(1, map.clone(), vec![clip(1.0), clip(4.5)], SR);
    assert!(remote.send(Box::new(score)).is_ok());
    for cmd in [
        Command::SetMasterGain { linear: 1.0 },
        Command::SetSong(Box::new(Song::new(Vec::new(), vec![TrackMix::default()]))),
        Command::SetTempoMap(Box::new(map.clone())),
        Command::SetPlugins {
            track: 0,
            plugins: Some(Box::new(PluginTrack::new(
                Some(Box::new(player)),
                Vec::new(),
                BLOCK,
            ))),
        },
        Command::SetPosition { beat: 0.0 },
        Command::Play,
    ] {
        assert!(tx.push(cmd).is_ok());
    }
    let mut buf = vec![0f32; BLOCK * 2];
    let mut out = Vec::with_capacity(SR as usize * 12);
    // A second score (the second phrase moved to 5 s), made here and sent while playing.
    let moved = Box::new(SingerScore::new(2, map, vec![clip(1.0), clip(5.0)], SR));
    let mut moved = Some(moved);
    let blocks = (SR as usize * 6) / BLOCK;
    for k in 0..blocks {
        if k == blocks / 2 {
            // At 3 s: before either place of the second phrase.
            if let Some(s) = moved.take() {
                assert!(remote.send(s).is_ok());
            }
        }
        assert_no_alloc::assert_no_alloc(|| core.process(&mut buf));
        out.extend_from_slice(&buf);
        remote.collect();
        while grx.pop().is_ok() {}
    }
    // The instrument is heard one block late (the engine's plug-in delay).
    let at = |secs: f64| (secs * SR as f64).round() as usize + BLOCK;
    // To the sample (one either way for the tempo map's rounding).
    let near = |got: usize, want: usize| got.abs_diff(want) <= 1;
    let first = first_sound(&out).expect("sound");
    assert!(near(first, at(1.0)), "first phrase at 1 s: {first}");
    let second = first_sound(&out[2 * (at(1.0) + SR as usize / 5)..])
        .map(|i| i + at(1.0) + SR as usize / 5)
        .expect("second phrase");
    assert!(
        near(second, at(5.0)),
        "moved phrase at 5 s (beat 7): {second}"
    );
    assert_eq!(
        remote
            .status
            .playing
            .load(std::sync::atomic::Ordering::Relaxed),
        2
    );
}
