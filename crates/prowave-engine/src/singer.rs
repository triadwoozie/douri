//! The Singer instrument's audio-thread part: it plays the phrases its track's notes and lyrics
//! were sung into (offline, by OpenUtau or DiffSinger, in the app) at their places in the song,
//! following the transport.
//!
//! A singer cannot sing a note as it is played (it needs the whole phrase, and synthesis is
//! slower than real time), so the app sings each phrase ahead of time and sends the finished
//! sounds here as a [`SingerScore`]: every phrase's sound at the engine's rate and the song
//! second its first sample belongs at, with the tempo map the seconds were read from. For each
//! frame the transport's beat becomes a song second through that map, and every phrase that
//! covers that second is mixed in. A score is replaced whole through a lock-free ring; the
//! one it replaces goes back through another ring to be freed on the UI thread, so
//! [`SingerPlayer::process`] neither allocates, frees nor locks.
//!
//! The sound goes through the track's channel strip like any instrument's (inserts, sends,
//! fader, pan, automation). MIDI played into it live is not sung.
//!
//! Logic Pro has no equivalent; the nearest are Synthesizer V and ACE Studio in a DAW.

use crate::tempo_map::TempoMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Scores waiting in the ring (the newest wins).
const RING: usize = 16;
/// Scores waiting to be freed on the UI thread.
const TRASH: usize = 64;

/// One sung phrase: its sound (mono, at the engine's rate) and the song second of its first
/// sample.
#[derive(Clone, Debug)]
pub struct SingerClip {
    pub origin: f64,
    pub sound: Arc<[f32]>,
}

/// Everything a Singer plays: its phrases in song seconds, and the tempo map that turns the
/// transport's beats into those seconds.
#[derive(Clone, Debug)]
pub struct SingerScore {
    /// Tells scores apart (the app's key of what it holds).
    pub id: u64,
    pub map: TempoMap,
    /// By `origin`.
    pub clips: Vec<SingerClip>,
    /// The longest clip, in seconds (how far back a clip that still sounds can start).
    pub longest: f64,
}

impl SingerScore {
    /// `rate`: the sample rate of the clips' sounds (the engine's).
    pub fn new(id: u64, map: TempoMap, mut clips: Vec<SingerClip>, rate: u32) -> Self {
        clips.retain(|c| c.origin.is_finite() && !c.sound.is_empty());
        clips.sort_by(|a, b| a.origin.total_cmp(&b.origin));
        let longest = clips
            .iter()
            .map(|c| c.sound.len() as f64 / f64::from(rate.max(1)))
            .fold(0.0, f64::max);
        Self {
            id,
            map,
            clips,
            longest,
        }
    }

    pub fn clips(&self) -> &[SingerClip] {
        &self.clips
    }

    /// The sum of the clips at song second `t` (linear interpolation between samples).
    pub fn at(&self, t: f64, rate: f64) -> f32 {
        let hi = self.clips.partition_point(|c| c.origin < t + 1.0 / rate);
        let from = t - self.longest - 1.0 / rate;
        let lo = self.clips[..hi].partition_point(|c| c.origin < from);
        let mut acc = 0.0f32;
        for c in &self.clips[lo..hi] {
            let pos = (t - c.origin) * rate;
            let k = pos.floor();
            if k < 0.0 {
                // Just before the first sample (silence before it).
                if pos > -1.0 {
                    acc += c.sound[0] * (1.0 + pos) as f32;
                }
                continue;
            }
            let k = k as usize;
            let frac = (pos - k as f64) as f32;
            let s = &c.sound;
            if k + 1 < s.len() {
                acc += s[k] + (s[k + 1] - s[k]) * frac;
            } else if k < s.len() {
                acc += s[k] * (1.0 - frac);
            }
        }
        acc
    }
}

/// What the UI can read of a running Singer.
#[derive(Debug, Default)]
pub struct SingerStatus {
    /// The id of the score it plays (0: none yet).
    pub playing: AtomicU64,
}

/// The UI's end of a Singer.
pub struct SingerRemote {
    tx: rtrb::Producer<Box<SingerScore>>,
    trash: rtrb::Consumer<Box<SingerScore>>,
    pub status: Arc<SingerStatus>,
    rate: u32,
}

impl SingerRemote {
    /// Sends `score` to play from now on; when the ring is full it comes back.
    #[allow(clippy::result_large_err)]
    pub fn send(&mut self, score: Box<SingerScore>) -> Result<(), Box<SingerScore>> {
        self.collect();
        self.tx.push(score).map_err(|rtrb::PushError::Full(s)| s)
    }

    /// Frees the scores the audio thread is done with.
    pub fn collect(&mut self) {
        while self.trash.pop().is_ok() {}
    }

    /// The engine's sample rate (the rate the clips' sounds must have).
    pub fn sample_rate(&self) -> u32 {
        self.rate
    }

    pub fn is_abandoned(&self) -> bool {
        self.tx.is_abandoned()
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }
}

/// Plays the score on the audio thread.
pub struct SingerPlayer {
    rx: rtrb::Consumer<Box<SingerScore>>,
    trash: rtrb::Producer<Box<SingerScore>>,
    status: Arc<SingerStatus>,
    held: Option<Box<SingerScore>>,
    score: Option<Box<SingerScore>>,
    rate: f64,
    beats: Box<[f64]>,
    n_beats: usize,
}

impl SingerPlayer {
    /// `max_block`: the engine's process block size (up to `MAX_BLOCK_FRAMES`).
    pub fn new(sample_rate: u32, max_block: usize) -> (Self, SingerRemote) {
        let (tx, rx) = rtrb::RingBuffer::new(RING);
        let (trash_tx, trash_rx) = rtrb::RingBuffer::new(TRASH);
        let status = Arc::new(SingerStatus::default());
        (
            Self {
                rx,
                trash: trash_tx,
                status: status.clone(),
                held: None,
                score: None,
                rate: f64::from(sample_rate.max(1)),
                beats: vec![f64::NAN; max_block].into_boxed_slice(),
                n_beats: 0,
            },
            SingerRemote {
                tx,
                trash: trash_rx,
                status,
                rate: sample_rate,
            },
        )
    }

    /// Takes the newest score from the ring, handing the one it replaces back to be freed.
    pub fn receive(&mut self) {
        if let Some(old) = self.held.take() {
            if let Err(rtrb::PushError::Full(old)) = self.trash.push(old) {
                self.held = Some(old);
                return;
            }
        }
        while let Ok(new) = self.rx.pop() {
            self.status.playing.store(new.id, Ordering::Relaxed);
            if let Some(old) = self.score.replace(new) {
                if let Err(rtrb::PushError::Full(old)) = self.trash.push(old) {
                    self.held = Some(old);
                    return;
                }
            }
        }
    }

    pub fn transport(&mut self, beats: &[f64]) {
        let n = beats.len().min(self.beats.len());
        self.beats[..n].copy_from_slice(&beats[..n]);
        self.n_beats = n;
    }

    pub fn process(&mut self, out: (&mut [f32], &mut [f32])) {
        self.receive();
        let (l, r) = out;
        let n = l.len().min(r.len());
        match &self.score {
            Some(sc) if !sc.clips.is_empty() => {
                for i in 0..n {
                    let b = if i < self.n_beats {
                        self.beats[i]
                    } else {
                        f64::NAN
                    };
                    let v = if b.is_finite() {
                        sc.at(sc.map.seconds_at(b), self.rate)
                    } else {
                        0.0
                    };
                    l[i] = v;
                    r[i] = v;
                }
            }
            _ => {
                l[..n].fill(0.0);
                r[..n].fill(0.0);
            }
        }
        self.n_beats = 0;
    }

    pub fn latency_seconds(&self) -> f64 {
        0.0
    }

    pub fn wants_transport(&self) -> bool {
        true
    }
}

impl crate::plugin_track::BlockProcessor for SingerPlayer {
    fn midi(&mut self, _status: u8, _data1: u8, _data2: u8, _offset: u32) {}
    fn process(&mut self, _input: (&[f32], &[f32]), out: (&mut [f32], &mut [f32])) {
        self.process(out);
    }
    fn latency_seconds(&self) -> f64 {
        0.0
    }
    fn wants_transport(&self) -> bool {
        true
    }
    fn transport(&mut self, beats: &[f64]) {
        self.transport(beats);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(bpm: f32) -> TempoMap {
        TempoMap::constant(bpm, (4, 4))
    }

    #[test]
    fn clips_play_at_their_song_seconds_and_overlaps_are_summed() {
        let rate = 1000;
        let (mut p, mut remote) = SingerPlayer::new(rate, 64);
        let a: Arc<[f32]> = vec![1.0; 10].into();
        let b: Arc<[f32]> = vec![0.5; 10].into();
        let score = SingerScore::new(
            9,
            map(60.0),
            vec![
                SingerClip {
                    origin: 0.105,
                    sound: b,
                },
                SingerClip {
                    origin: 0.100,
                    sound: a,
                },
            ],
            rate,
        );
        assert!(remote.send(Box::new(score)).is_ok());
        // 60 BPM: beat = second. Frames 0..20 at 0.095..0.115 s.
        let beats: Vec<f64> = (0..20).map(|i| 0.095 + i as f64 / 1000.0).collect();
        p.transport(&beats);
        let (mut l, mut r) = (vec![0f32; 20], vec![0f32; 20]);
        p.process((&mut l, &mut r));
        let want: Vec<f32> = (0..20)
            .map(|i| {
                let t = 95 + i;
                let mut v = 0.0;
                if (100..110).contains(&t) {
                    v += 1.0;
                }
                if (105..115).contains(&t) {
                    v += 0.5;
                }
                v
            })
            .collect();
        for i in 0..20 {
            assert!((l[i] - want[i]).abs() < 1e-4, "{i}: {} {}", l[i], want[i]);
        }
        assert_eq!(l, r);
        assert_eq!(remote.status.playing.load(Ordering::Relaxed), 9);
        // Stopped (NaN beats) and no transport: silence.
        p.transport(&[f64::NAN; 20]);
        p.process((&mut l, &mut r));
        assert!(l.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn a_new_score_replaces_the_old_which_comes_back_to_be_freed() {
        let (mut p, mut remote) = SingerPlayer::new(1000, 64);
        let one = |id| Box::new(SingerScore::new(id, map(120.0), Vec::new(), 1000));
        assert!(remote.send(one(1)).is_ok());
        assert!(remote.send(one(2)).is_ok());
        let (mut l, mut r) = (vec![0f32; 8], vec![0f32; 8]);
        p.process((&mut l, &mut r));
        assert_eq!(remote.status.playing.load(Ordering::Relaxed), 2);
        assert!(remote.trash.pop().is_ok(), "score 1 handed back");
        assert!(remote.trash.pop().is_err());
    }
}
