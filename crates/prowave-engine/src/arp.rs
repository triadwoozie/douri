//! The Arpeggiator's note generator: the keys held on a track go in, the notes of the step
//! pattern come out, each with the beat it starts or ends on.
//!
//! One generator serves both ways a track is played. The notes in a track's regions are run
//! through it ahead of time ([`render`]), so the engine, the bounce and a MIDI file all get the
//! same notes. The keys played live go through an [`ArpRunner`] the engine keeps for the track
//! and clocks frame by frame: by the song position while the song plays, by the project tempo
//! while it is stopped. Nothing here allocates except [`render`], which is not called on the
//! audio thread.
use crate::core::NoteEvent;

/// Most steps in a pattern.
pub const ARP_STEPS: usize = 32;
/// Most keys held at once; more are not taken.
pub const ARP_KEYS: usize = 16;
/// Most generated notes sounding at once; the one ending first makes room for another.
const SOUNDING: usize = 32;
/// A key pressed this far into a step (as a part of the step) still plays that step, at once.
const LATE: f64 = 0.3;
/// A clock that moves back, or forward by more than this many beats, has jumped.
const JUMP: f64 = 0.5;

/// How long a step is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArpRate {
    QuarterDotted,
    Quarter,
    QuarterTriplet,
    EighthDotted,
    Eighth,
    EighthTriplet,
    SixteenthDotted,
    Sixteenth,
    SixteenthTriplet,
    ThirtySecondDotted,
    ThirtySecond,
    ThirtySecondTriplet,
}

impl ArpRate {
    pub const ALL: [ArpRate; 12] = [
        Self::QuarterDotted,
        Self::Quarter,
        Self::QuarterTriplet,
        Self::EighthDotted,
        Self::Eighth,
        Self::EighthTriplet,
        Self::SixteenthDotted,
        Self::Sixteenth,
        Self::SixteenthTriplet,
        Self::ThirtySecondDotted,
        Self::ThirtySecond,
        Self::ThirtySecondTriplet,
    ];

    /// The step in quarter notes.
    pub fn beats(self) -> f64 {
        match self {
            Self::QuarterDotted => 1.5,
            Self::Quarter => 1.0,
            Self::QuarterTriplet => 2.0 / 3.0,
            Self::EighthDotted => 0.75,
            Self::Eighth => 0.5,
            Self::EighthTriplet => 1.0 / 3.0,
            Self::SixteenthDotted => 0.375,
            Self::Sixteenth => 0.25,
            Self::SixteenthTriplet => 1.0 / 6.0,
            Self::ThirtySecondDotted => 0.1875,
            Self::ThirtySecond => 0.125,
            Self::ThirtySecondTriplet => 1.0 / 12.0,
        }
    }

    /// How it is shown.
    pub fn name(self) -> &'static str {
        match self {
            Self::QuarterDotted => "1/4.",
            Self::Quarter => "1/4",
            Self::QuarterTriplet => "1/4T",
            Self::EighthDotted => "1/8.",
            Self::Eighth => "1/8",
            Self::EighthTriplet => "1/8T",
            Self::SixteenthDotted => "1/16.",
            Self::Sixteenth => "1/16",
            Self::SixteenthTriplet => "1/16T",
            Self::ThirtySecondDotted => "1/32.",
            Self::ThirtySecond => "1/32",
            Self::ThirtySecondTriplet => "1/32T",
        }
    }

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|r| *r == self).unwrap_or(7) as u8
    }

    pub fn from_index(i: u8) -> Self {
        Self::ALL
            .get(i as usize)
            .copied()
            .unwrap_or(Self::Sixteenth)
    }
}

/// The order the held keys are played in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArpMode {
    Up,
    Down,
    UpDown,
    DownUp,
    Random,
    /// The order the keys were pressed in.
    AsPlayed,
    /// Every step plays all the held keys.
    Chord,
}

impl ArpMode {
    pub const ALL: [ArpMode; 7] = [
        Self::Up,
        Self::Down,
        Self::UpDown,
        Self::DownUp,
        Self::Random,
        Self::AsPlayed,
        Self::Chord,
    ];

    pub fn index(self) -> u8 {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0) as u8
    }

    pub fn from_index(i: u8) -> Self {
        Self::ALL.get(i as usize).copied().unwrap_or(Self::Up)
    }
}

/// One step of the pattern.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArpStep {
    /// Off: a rest.
    pub on: bool,
    /// The step's velocity bar, 1 - 127.
    pub vel: u8,
    /// The step's note lasts through the next step, which plays nothing of its own.
    pub tie: bool,
    /// The step plays all the held keys at once.
    pub chord: bool,
    /// Semitones added to what the step plays, -24 - 24.
    pub transpose: i8,
}

impl Default for ArpStep {
    fn default() -> Self {
        Self {
            on: true,
            vel: 100,
            tie: false,
            chord: false,
            transpose: 0,
        }
    }
}

/// Everything the Arpeggiator is set to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArpSettings {
    pub rate: ArpRate,
    pub mode: ArpMode,
    /// 1 - 4: how many octaves the held keys are played over.
    pub octaves: u8,
    /// 5 - 200 percent of a step.
    pub gate: u8,
    /// 0 - 100: every second step is late by up to half a step (67 is a triplet feel).
    pub swing: u8,
    /// 0 - 100: how much of a note's velocity comes from its step's bar (0: all from the key).
    pub step_velocity: u8,
    /// Keys stay held after they are let go, until new ones are pressed.
    pub latch: bool,
    /// On: the steps sit on the song's grid, whenever the keys are pressed. Off: the pattern
    /// starts again from its first step with every new chord.
    pub sync: bool,
    /// 1 - 32: the steps played before the pattern starts again.
    pub length: u8,
    /// What Random draws from.
    pub seed: u32,
    pub steps: [ArpStep; ARP_STEPS],
}

impl Default for ArpSettings {
    fn default() -> Self {
        Self {
            rate: ArpRate::Sixteenth,
            mode: ArpMode::Up,
            octaves: 1,
            gate: 80,
            swing: 0,
            step_velocity: 100,
            latch: false,
            sync: false,
            length: 16,
            seed: 1,
            steps: [ArpStep::default(); ARP_STEPS],
        }
    }
}

impl ArpSettings {
    /// The same settings with every value inside its range.
    pub fn sanitized(mut self) -> Self {
        self.octaves = self.octaves.clamp(1, 4);
        self.gate = self.gate.clamp(5, 200);
        self.swing = self.swing.min(100);
        self.step_velocity = self.step_velocity.min(100);
        self.length = self.length.clamp(1, ARP_STEPS as u8);
        for s in &mut self.steps {
            s.vel = s.vel.clamp(1, 127);
            s.transpose = s.transpose.clamp(-24, 24);
        }
        self
    }
}

/// A generated note starting or ending.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArpEvent {
    pub beat: f64,
    pub on: bool,
    pub pitch: u8,
    pub vel: u8,
}

/// The state of one track's arpeggiator: the keys held, the notes sounding and where the
/// pattern is.
#[derive(Clone, Copy, Debug)]
pub struct ArpRunner {
    /// The held keys (pitch, velocity) in the order they were pressed.
    held: [(u8, u8); ARP_KEYS],
    n_held: usize,
    /// The keys that are down now, by pitch (with Latch a key can be held and not down).
    down: u128,
    /// The generated notes that sound: pitch and the beat each ends on.
    sounding: [(u8, f64); SOUNDING],
    n_sounding: usize,
    /// The beat of step 0.
    anchor: f64,
    /// The beat the chord began on: none of its steps play before it.
    floor: f64,
    /// The next step to play, counted from the anchor.
    next_step: i64,
    /// The last step played (so a chord pressed again in the same step does not repeat it).
    last_fired: i64,
    /// Notes played since the chord began.
    counter: u32,
    /// Chords begun, for Random.
    chord_id: u32,
    /// Steps still covered by a tied note.
    skip: u8,
    /// The clock at the last [`ArpRunner::clock`] (NaN: none yet).
    last: f64,
}

impl Default for ArpRunner {
    fn default() -> Self {
        Self {
            held: [(0, 0); ARP_KEYS],
            n_held: 0,
            down: 0,
            sounding: [(0, 0.0); SOUNDING],
            n_sounding: 0,
            anchor: 0.0,
            floor: 0.0,
            next_step: 0,
            last_fired: i64::MIN,
            counter: 0,
            chord_id: 0,
            skip: 0,
            last: f64::NAN,
        }
    }
}

/// The same numbers every time, well mixed.
fn hash(seed: u32, a: u32, b: u32) -> u64 {
    let mut x = (u64::from(seed) << 32 | u64::from(a))
        .wrapping_add(u64::from(b).wrapping_mul(0x9e37_79b9_7f4a_7c15));
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn mix_velocity(key: u8, step: u8, amount: u8) -> u8 {
    let a = f32::from(amount.min(100)) / 100.0;
    (f32::from(key) * (1.0 - a) + f32::from(step) * a)
        .round()
        .clamp(1.0, 127.0) as u8
}

impl ArpRunner {
    /// Nothing held and nothing sounding.
    pub fn idle(&self) -> bool {
        self.n_held == 0 && self.n_sounding == 0
    }

    /// How many keys are held.
    pub fn held(&self) -> usize {
        self.n_held
    }

    /// The beat step `k` starts on.
    fn step_time(&self, s: &ArpSettings, k: i64) -> f64 {
        let step = s.rate.beats();
        let swing = if k.rem_euclid(2) == 1 {
            f64::from(s.swing.min(100)) / 100.0 * 0.5 * step
        } else {
            0.0
        };
        self.anchor + k as f64 * step + swing
    }

    /// A chord begins at `now`: the notes start from the first, and the pattern either starts
    /// again or is picked up where the song is.
    fn begin_chord(&mut self, s: &ArpSettings, now: f64) {
        self.counter = 0;
        self.skip = 0;
        self.chord_id = self.chord_id.wrapping_add(1);
        self.floor = now;
        if s.sync {
            self.anchor = 0.0;
            let step = s.rate.beats();
            let k0 = (now / step).floor() as i64;
            let t0 = self.step_time(s, k0);
            let k = if t0 > now || now - t0 <= LATE * step {
                k0
            } else {
                k0 + 1
            };
            self.next_step = k.max(self.last_fired.saturating_add(1));
        } else {
            self.anchor = now;
            self.next_step = 0;
            self.last_fired = -1;
        }
    }

    /// A key goes down at `now`.
    pub fn key_down(&mut self, s: &ArpSettings, pitch: u8, vel: u8, now: f64) {
        let pitch = pitch.min(127);
        // With Latch, the first key after all were let go begins a new chord.
        if s.latch && self.down == 0 {
            self.n_held = 0;
        }
        self.down |= 1u128 << pitch;
        let fresh = self.n_held == 0;
        if let Some(h) = self.held[..self.n_held].iter_mut().find(|h| h.0 == pitch) {
            h.1 = vel;
        } else if self.n_held < ARP_KEYS {
            self.held[self.n_held] = (pitch, vel);
            self.n_held += 1;
        }
        if fresh {
            self.begin_chord(s, now);
        }
    }

    /// A key goes up. With Latch it stays held.
    pub fn key_up(&mut self, s: &ArpSettings, pitch: u8) {
        let pitch = pitch.min(127);
        self.down &= !(1u128 << pitch);
        if !s.latch {
            self.drop_key(pitch);
        }
    }

    fn drop_key(&mut self, pitch: u8) {
        if let Some(i) = self.held[..self.n_held].iter().position(|h| h.0 == pitch) {
            self.held.copy_within(i + 1..self.n_held, i);
            self.n_held -= 1;
        }
    }

    /// Latch was switched off: the keys that are not down are let go.
    pub fn unlatch(&mut self) {
        let mut i = 0;
        while i < self.n_held {
            let p = self.held[i].0;
            if self.down & (1u128 << p) == 0 {
                self.drop_key(p);
            } else {
                i += 1;
            }
        }
    }

    /// Ends every sounding note at `now` and lets every key go.
    pub fn stop<F: FnMut(ArpEvent)>(&mut self, now: f64, out: &mut F) {
        self.silence(now, out);
        self.n_held = 0;
        self.down = 0;
    }

    fn silence<F: FnMut(ArpEvent)>(&mut self, now: f64, out: &mut F) {
        for &(pitch, _) in &self.sounding[..self.n_sounding] {
            out(ArpEvent {
                beat: now,
                on: false,
                pitch,
                vel: 0,
            });
        }
        self.n_sounding = 0;
    }

    /// The clock jumped to `now` (the song was started, stopped or moved), or the step length
    /// changed: what sounds ends, and a held chord begins again from there.
    pub fn jump<F: FnMut(ArpEvent)>(&mut self, s: &ArpSettings, now: f64, out: &mut F) {
        self.silence(now, out);
        self.last_fired = i64::MIN;
        if self.n_held > 0 {
            self.begin_chord(s, now);
        }
        self.last = now;
    }

    /// The clock is at `now`: plays everything due up to and including it. A clock that does
    /// not carry on from the last call has jumped.
    pub fn clock<F: FnMut(ArpEvent)>(&mut self, s: &ArpSettings, now: f64, out: &mut F) {
        if !now.is_finite() {
            return;
        }
        if self.idle() {
            self.last = now;
            return;
        }
        if !self.last.is_finite() || now < self.last - 1e-6 || now - self.last > JUMP {
            self.jump(s, now, out);
        }
        self.advance(s, now, true, out);
        self.last = now;
    }

    /// Tells the runner where the clock is without playing anything (before a key goes down).
    pub fn set_clock(&mut self, now: f64) {
        if self.idle() {
            self.last = now;
        }
    }

    /// Plays every step and ends every note due before `to` (or on it, when `inclusive`), in
    /// the order they fall.
    pub fn advance<F: FnMut(ArpEvent)>(
        &mut self,
        s: &ArpSettings,
        to: f64,
        inclusive: bool,
        out: &mut F,
    ) {
        let due = |t: f64| if inclusive { t <= to } else { t < to };
        if s.rate.beats() <= 0.0 {
            return;
        }
        loop {
            let mut first_off: Option<(usize, f64)> = None;
            for (i, &(_, at)) in self.sounding[..self.n_sounding].iter().enumerate() {
                if first_off.is_none_or(|(_, t)| at < t) {
                    first_off = Some((i, at));
                }
            }
            let step = (self.n_held > 0).then(|| self.step_time(s, self.next_step).max(self.floor));
            match (first_off, step) {
                // A note that ends where a step starts ends first, so the same key can be
                // played again.
                (Some((i, at)), st) if st.is_none_or(|t| at <= t) => {
                    if !due(at) {
                        break;
                    }
                    let pitch = self.sounding[i].0;
                    self.n_sounding -= 1;
                    self.sounding[i] = self.sounding[self.n_sounding];
                    out(ArpEvent {
                        beat: at,
                        on: false,
                        pitch,
                        vel: 0,
                    });
                }
                (_, Some(t)) => {
                    if !due(t) {
                        break;
                    }
                    self.play_step(s, t, out);
                }
                _ => break,
            }
        }
    }

    /// Plays the next step, at beat `t`.
    fn play_step<F: FnMut(ArpEvent)>(&mut self, s: &ArpSettings, t: f64, out: &mut F) {
        let k = self.next_step;
        self.next_step = k + 1;
        self.last_fired = k;
        let len = usize::from(s.length.clamp(1, ARP_STEPS as u8));
        let idx = k.rem_euclid(len as i64) as usize;
        if self.skip > 0 {
            self.skip -= 1;
            return;
        }
        let st = s.steps[idx];
        if !st.on {
            return;
        }
        // A tied step lasts through the steps it is tied into.
        let mut run = 1usize;
        while run < len && s.steps[(idx + run - 1) % len].tie {
            run += 1;
        }
        self.skip = (run - 1) as u8;
        let last_start = self.step_time(s, k + run as i64 - 1).max(t);
        let off = last_start + s.rate.beats() * f64::from(s.gate.clamp(5, 200)) / 100.0;
        let transpose = i32::from(st.transpose);
        let octaves = u32::from(s.octaves.clamp(1, 4));
        if st.chord || s.mode == ArpMode::Chord {
            let shift = if s.mode == ArpMode::Chord {
                12 * (self.counter % octaves) as i32
            } else {
                0
            };
            let held = self.held;
            for &(p, v) in &held[..self.n_held] {
                let vel = mix_velocity(v, st.vel, s.step_velocity);
                self.note_on(i32::from(p) + shift + transpose, vel, t, off, out);
            }
        } else if let Some((p, v)) = self.pick(s) {
            let vel = mix_velocity(v, st.vel, s.step_velocity);
            self.note_on(i32::from(p) + transpose, vel, t, off, out);
        }
        self.counter = self.counter.wrapping_add(1);
    }

    /// The key (pitch with its octave, velocity) the next note plays.
    fn pick(&self, s: &ArpSettings) -> Option<(u8, u8)> {
        let octaves = usize::from(s.octaves.clamp(1, 4));
        let mut keys = [(0u8, 0u8); ARP_KEYS * 4];
        let mut m = 0;
        for o in 0..octaves {
            for &(p, v) in &self.held[..self.n_held] {
                let q = usize::from(p) + 12 * o;
                if q <= 127 && !keys[..m].iter().any(|k| usize::from(k.0) == q) {
                    keys[m] = (q as u8, v);
                    m += 1;
                }
            }
        }
        if m == 0 {
            return None;
        }
        if s.mode != ArpMode::AsPlayed {
            keys[..m].sort_unstable_by_key(|k| k.0);
        }
        let c = self.counter as usize;
        let bounce = |c: usize| {
            if m <= 1 {
                0
            } else {
                let cycle = 2 * m - 2;
                let j = c % cycle;
                if j < m {
                    j
                } else {
                    cycle - j
                }
            }
        };
        let i = match s.mode {
            ArpMode::Up | ArpMode::AsPlayed | ArpMode::Chord => c % m,
            ArpMode::Down => m - 1 - c % m,
            ArpMode::UpDown => bounce(c),
            ArpMode::DownUp => m - 1 - bounce(c),
            ArpMode::Random => (hash(s.seed, self.chord_id, self.counter) % m as u64) as usize,
        };
        Some(keys[i])
    }

    fn note_on<F: FnMut(ArpEvent)>(&mut self, pitch: i32, vel: u8, t: f64, off: f64, out: &mut F) {
        if !(0..=127).contains(&pitch) {
            return;
        }
        let pitch = pitch as u8;
        // The same key again: the note before it ends first.
        let mut end = self.sounding[..self.n_sounding]
            .iter()
            .position(|n| n.0 == pitch);
        if end.is_none() && self.n_sounding == SOUNDING {
            let mut first = 0;
            for i in 1..self.n_sounding {
                if self.sounding[i].1 < self.sounding[first].1 {
                    first = i;
                }
            }
            end = Some(first);
        }
        if let Some(i) = end {
            let old = self.sounding[i].0;
            self.n_sounding -= 1;
            self.sounding[i] = self.sounding[self.n_sounding];
            out(ArpEvent {
                beat: t,
                on: false,
                pitch: old,
                vel: 0,
            });
        }
        self.sounding[self.n_sounding] = (pitch, off);
        self.n_sounding += 1;
        out(ArpEvent {
            beat: t,
            on: true,
            pitch,
            vel,
        });
    }
}

/// Makes notes of the generator's note starts and ends.
struct Collector {
    out: Vec<NoteEvent>,
    /// The notes that have started: pitch, start, velocity.
    open: Vec<(u8, f64, u8)>,
    track: u16,
    max_notes: usize,
}

impl Collector {
    fn take(&mut self, e: ArpEvent) {
        if e.on {
            self.open.push((e.pitch, e.beat, e.vel));
        } else if let Some(i) = self.open.iter().position(|o| o.0 == e.pitch) {
            let (pitch, start, vel) = self.open.swap_remove(i);
            if self.out.len() < self.max_notes {
                self.out.push(NoteEvent {
                    start_beat: start,
                    end_beat: e.beat.max(start + 1e-3),
                    pitch,
                    vel,
                    track: self.track,
                });
            }
        }
    }
}

/// What the Arpeggiator makes of `notes` (one track's, in any order): the notes held together
/// are its keys. With Latch the last keys play on until `end`. At most `max_notes` come back,
/// sorted by start.
pub fn render(s: &ArpSettings, notes: &[NoteEvent], end: f64, max_notes: usize) -> Vec<NoteEvent> {
    let s = s.sanitized();
    let Some(track) = notes.first().map(|n| n.track) else {
        return Vec::new();
    };
    // (beat, down, pitch, velocity), a key going up before one going down on the same beat.
    let mut keys: Vec<(f64, bool, u8, u8)> = Vec::with_capacity(notes.len() * 2);
    for n in notes {
        if !(n.start_beat.is_finite() && n.end_beat.is_finite()) {
            continue;
        }
        let pitch = n.pitch.min(127);
        keys.push((n.start_beat, true, pitch, n.vel.clamp(1, 127)));
        keys.push((n.end_beat.max(n.start_beat + 1e-6), false, pitch, 0));
    }
    keys.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let last_key = keys.last().map_or(0.0, |k| k.0);
    let mut made = Collector {
        out: Vec::new(),
        open: Vec::new(),
        track,
        max_notes,
    };
    let mut run = ArpRunner::default();
    // Notes of the same pitch can overlap: the key is up when the last of them ends.
    let mut count = [0u16; 128];
    for &(beat, down, pitch, vel) in &keys {
        if made.out.len() >= max_notes {
            break;
        }
        run.advance(&s, beat, false, &mut |e| made.take(e));
        let c = &mut count[usize::from(pitch)];
        if down {
            *c = c.saturating_add(1);
            run.key_down(&s, pitch, vel, beat);
        } else {
            *c = c.saturating_sub(1);
            if *c == 0 {
                run.key_up(&s, pitch);
            }
        }
    }
    if made.out.len() >= max_notes {
        run.stop(last_key, &mut |e| made.take(e));
    } else if run.held() > 0 {
        // Latched keys play on to the end (a sane one).
        let end = if end.is_finite() { end } else { last_key };
        let end = end.clamp(last_key, last_key + 100_000.0);
        run.advance(&s, end, false, &mut |e| made.take(e));
        run.stop(end, &mut |e| made.take(e));
    } else {
        run.advance(&s, f64::INFINITY, false, &mut |e| made.take(e));
    }
    let mut out = made.out;
    out.sort_by(|a, b| {
        a.start_beat
            .total_cmp(&b.start_beat)
            .then(a.pitch.cmp(&b.pitch))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(start: f64, end: f64, pitch: u8) -> NoteEvent {
        NoteEvent {
            start_beat: start,
            end_beat: end,
            pitch,
            vel: 90,
            track: 4,
        }
    }

    fn chord(start: f64, end: f64) -> Vec<NoteEvent> {
        vec![
            note(start, end, 64),
            note(start, end, 60),
            note(start, end, 67),
        ]
    }

    fn quarter(mode: ArpMode) -> ArpSettings {
        ArpSettings {
            rate: ArpRate::Quarter,
            mode,
            gate: 50,
            ..ArpSettings::default()
        }
    }

    fn pitches(notes: &[NoteEvent]) -> Vec<u8> {
        notes.iter().map(|n| n.pitch).collect()
    }

    fn starts(notes: &[NoteEvent]) -> Vec<f64> {
        notes.iter().map(|n| n.start_beat).collect()
    }

    fn close(a: &[f64], b: &[f64]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn every_mode_plays_the_held_keys_in_its_order() {
        let play = |mode| pitches(&render(&quarter(mode), &chord(0.0, 8.0), 8.0, 1000));
        assert_eq!(play(ArpMode::Up), [60, 64, 67, 60, 64, 67, 60, 64]);
        assert_eq!(play(ArpMode::Down), [67, 64, 60, 67, 64, 60, 67, 64]);
        assert_eq!(play(ArpMode::UpDown), [60, 64, 67, 64, 60, 64, 67, 64]);
        assert_eq!(play(ArpMode::DownUp), [67, 64, 60, 64, 67, 64, 60, 64]);
        // As Played: the order the keys went down in (here the order of the notes).
        let mut later = chord(0.0, 8.0);
        later[1].start_beat = 0.01;
        later[2].start_beat = 0.02;
        let played = render(&quarter(ArpMode::AsPlayed), &later, 8.0, 1000);
        assert_eq!(pitches(&played)[..6], [64, 60, 67, 64, 60, 67]);
        // Chord: all of them on every step.
        let all = render(&quarter(ArpMode::Chord), &chord(0.0, 2.0), 2.0, 1000);
        assert_eq!(pitches(&all), [60, 64, 67, 60, 64, 67]);
        assert!(close(&starts(&all), &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0]));
        // Random: the held keys only, the same every time, and not simply in order.
        let a = play(ArpMode::Random);
        assert_eq!(a, play(ArpMode::Random));
        assert!(a.iter().all(|p| [60, 64, 67].contains(p)));
        let long = pitches(&render(
            &ArpSettings {
                rate: ArpRate::Sixteenth,
                mode: ArpMode::Random,
                ..ArpSettings::default()
            },
            &chord(0.0, 16.0),
            16.0,
            1000,
        ));
        for p in [60, 64, 67] {
            assert!(long.contains(&p), "{p} is drawn some time");
        }
        assert_ne!(long[..6], [60, 64, 67, 60, 64, 67]);
        // One key and no keys.
        let one = render(&quarter(ArpMode::UpDown), &[note(0.0, 3.0, 50)], 3.0, 1000);
        assert_eq!(pitches(&one), [50, 50, 50]);
        assert!(render(&quarter(ArpMode::Up), &[], 8.0, 1000).is_empty());
    }

    #[test]
    fn the_octave_range_repeats_the_keys_higher_and_leaves_out_what_is_too_high() {
        let mut s = quarter(ArpMode::Up);
        s.octaves = 2;
        let up = render(&s, &chord(0.0, 7.0), 7.0, 1000);
        assert_eq!(pitches(&up), [60, 64, 67, 72, 76, 79, 60]);
        s.mode = ArpMode::Down;
        s.octaves = 3;
        let down = render(&s, &chord(0.0, 4.0), 4.0, 1000);
        assert_eq!(pitches(&down), [91, 88, 84, 79]);
        // Keys more than an octave apart still go up in pitch order.
        s.mode = ArpMode::Up;
        s.octaves = 2;
        let wide = render(&s, &[note(0.0, 4.0, 60), note(0.0, 4.0, 76)], 4.0, 1000);
        assert_eq!(pitches(&wide), [60, 72, 76, 88]);
        // Four octaves from a high key: nothing above 127.
        s.octaves = 4;
        let high = render(&s, &[note(0.0, 4.0, 110)], 4.0, 1000);
        assert_eq!(pitches(&high), [110, 122, 110, 122]);
        // Chord mode climbs an octave a step.
        s.mode = ArpMode::Chord;
        s.octaves = 2;
        let climbing = render(&s, &[note(0.0, 3.0, 60), note(0.0, 3.0, 64)], 3.0, 1000);
        assert_eq!(pitches(&climbing), [60, 64, 72, 76, 60, 64]);
    }

    #[test]
    fn the_rate_and_gate_set_where_notes_start_and_end() {
        for rate in ArpRate::ALL {
            let s = ArpSettings {
                rate,
                gate: 50,
                ..ArpSettings::default()
            };
            let out = render(&s, &chord(0.0, 2.999), 3.0, 1000);
            let step = rate.beats();
            assert_eq!(out.len(), (2.999 / step).ceil() as usize, "{rate:?}");
            for (k, n) in out.iter().enumerate() {
                assert!((n.start_beat - k as f64 * step).abs() < 1e-9, "{rate:?}");
                assert!((n.end_beat - n.start_beat - step / 2.0).abs() < 1e-9);
            }
            assert_eq!(ArpRate::from_index(rate.index()), rate);
        }
        assert_eq!(ArpRate::EighthDotted.beats(), 0.75);
        assert!((ArpRate::EighthTriplet.beats() * 3.0 - 1.0).abs() < 1e-12);
        // A gate over 100 % overlaps the next note; the same key played again ends first.
        let s = ArpSettings {
            rate: ArpRate::Quarter,
            gate: 150,
            ..ArpSettings::default()
        };
        let out = render(&s, &chord(0.0, 3.0), 3.0, 1000);
        assert!((out[0].end_beat - 1.5).abs() < 1e-9);
        let one = render(&s, &[note(0.0, 3.0, 60)], 3.0, 1000);
        assert!(close(&starts(&one), &[0.0, 1.0, 2.0]));
        assert!((one[0].end_beat - 1.0).abs() < 1e-9, "cut by the next");
        assert!((one[2].end_beat - 3.5).abs() < 1e-9, "the last rings out");
    }

    #[test]
    fn swing_moves_every_second_step_later() {
        let mut s = ArpSettings {
            rate: ArpRate::Eighth,
            gate: 50,
            ..ArpSettings::default()
        };
        let at = |s: &ArpSettings| starts(&render(s, &chord(0.0, 2.0), 2.0, 1000));
        assert!(close(&at(&s), &[0.0, 0.5, 1.0, 1.5]));
        // 100 %: half a step late. 67 %: about a third (the triplet feel).
        s.swing = 100;
        assert!(close(&at(&s), &[0.0, 0.75, 1.0, 1.75]));
        s.swing = 50;
        assert!(close(&at(&s), &[0.0, 0.625, 1.0, 1.625]));
        s.swing = 67;
        let t = at(&s);
        assert!((t[1] - (0.5 + 0.67 * 0.25)).abs() < 1e-9);
        assert!((t[1] - 2.0 / 3.0).abs() < 0.002, "{}", t[1]);
        // Locked to the song, the late steps are the song's odd ones, wherever the chord is.
        s.swing = 100;
        s.sync = true;
        let late = starts(&render(&s, &chord(1.0, 3.0), 3.0, 1000));
        assert!(close(&late, &[1.0, 1.75, 2.0, 2.75]), "{late:?}");
        let odd = starts(&render(&s, &chord(0.5, 2.0), 2.0, 1000));
        assert!(close(&odd, &[0.75, 1.0, 1.75]), "{odd:?}");
    }

    #[test]
    fn steps_rest_tie_transpose_and_play_chords() {
        let mut s = quarter(ArpMode::Up);
        s.length = 4;
        // A rest does not use up a key.
        s.steps[1].on = false;
        let out = render(&s, &chord(0.0, 8.0), 8.0, 1000);
        assert_eq!(pitches(&out), [60, 64, 67, 60, 64, 67]);
        assert!(close(&starts(&out), &[0.0, 2.0, 3.0, 4.0, 6.0, 7.0]));
        // A tie: the note lasts through the next step, which plays nothing.
        let mut s = quarter(ArpMode::Up);
        s.length = 4;
        s.steps[0].tie = true;
        let out = render(&s, &chord(0.0, 8.0), 8.0, 1000);
        assert!(close(&starts(&out), &[0.0, 2.0, 3.0, 4.0, 6.0, 7.0]));
        assert_eq!(pitches(&out), [60, 64, 67, 60, 64, 67]);
        assert!((out[0].end_beat - 1.5).abs() < 1e-9, "a step and its gate");
        assert!((out[1].end_beat - 2.5).abs() < 1e-9);
        // Two ties in a row make one note of three steps, also into a rest.
        s.steps[1].tie = true;
        s.steps[2].on = false;
        let out = render(&s, &chord(0.0, 4.0), 4.0, 1000);
        assert!(close(&starts(&out), &[0.0, 3.0]));
        assert!((out[0].end_beat - 2.5).abs() < 1e-9);
        // Every step tied: one note a pattern, not an endless one.
        let mut s = quarter(ArpMode::Up);
        s.length = 2;
        s.steps[0].tie = true;
        s.steps[1].tie = true;
        let out = render(&s, &chord(0.0, 4.0), 4.0, 1000);
        assert!(close(&starts(&out), &[0.0, 2.0]));
        // Transpose moves a step's note; one that leaves the range is not played.
        let mut s = quarter(ArpMode::Up);
        s.length = 2;
        s.steps[1].transpose = 12;
        let out = render(&s, &chord(0.0, 4.0), 4.0, 1000);
        assert_eq!(pitches(&out), [60, 76, 67, 72]);
        s.steps[1].transpose = 24;
        let out = render(&s, &[note(0.0, 2.0, 120)], 2.0, 1000);
        assert_eq!(pitches(&out), [120]);
        // A chord step plays every held key, moved by its transpose.
        let mut s = quarter(ArpMode::Up);
        s.length = 2;
        s.steps[1].chord = true;
        s.steps[1].transpose = -12;
        let out = render(&s, &chord(0.0, 3.0), 3.0, 1000);
        assert_eq!(pitches(&out), [60, 48, 52, 55, 67]);
        // The length is where the pattern starts again.
        let mut s = quarter(ArpMode::Up);
        s.length = 3;
        s.steps[2].on = false;
        s.steps[3].on = false;
        let out = render(&s, &chord(0.0, 6.0), 6.0, 1000);
        assert!(close(&starts(&out), &[0.0, 1.0, 3.0, 4.0]));
    }

    #[test]
    fn velocity_comes_from_the_keys_the_bars_or_both() {
        let mut s = quarter(ArpMode::Up);
        s.steps[0].vel = 127;
        s.steps[1].vel = 30;
        let vels = |s: &ArpSettings| {
            render(s, &chord(0.0, 2.0), 2.0, 1000)
                .iter()
                .map(|n| n.vel)
                .collect::<Vec<_>>()
        };
        s.step_velocity = 100;
        assert_eq!(vels(&s), [127, 30]);
        s.step_velocity = 0;
        assert_eq!(vels(&s), [90, 90]);
        s.step_velocity = 50;
        assert_eq!(vels(&s), [109, 60]);
        // Each key keeps its own velocity.
        let mut keys = chord(0.0, 3.0);
        keys[1].vel = 20;
        s.step_velocity = 0;
        let out = render(&s, &keys, 3.0, 1000);
        assert_eq!(out.iter().map(|n| n.vel).collect::<Vec<_>>(), [20, 90, 90]);
    }

    #[test]
    fn latch_holds_the_keys_until_new_ones_are_pressed() {
        let mut s = quarter(ArpMode::Up);
        s.latch = true;
        // A short chord at 0, another at 4; the song's regions end at 8.
        let mut notes = chord(0.0, 0.5);
        notes.extend([note(4.0, 4.5, 50), note(4.0, 4.5, 53)]);
        let out = render(&s, &notes, 8.0, 1000);
        assert_eq!(pitches(&out), [60, 64, 67, 60, 50, 53, 50, 53]);
        assert!(close(
            &starts(&out),
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]
        ));
        assert!(out.iter().all(|n| n.end_beat <= 8.0 + 1e-9));
        // Without Latch the notes stop with the keys.
        s.latch = false;
        assert_eq!(pitches(&render(&s, &notes, 8.0, 1000)), [60, 50]);
        // A key added while the others are still down joins the chord; pressed after they
        // were let go, it replaces them.
        s.latch = true;
        let joined = render(
            &s,
            &[note(0.0, 2.0, 60), note(1.5, 2.0, 64), note(5.0, 5.5, 72)],
            8.0,
            1000,
        );
        assert_eq!(pitches(&joined), [60, 60, 60, 64, 60, 72, 72, 72]);
        // The runner lets latched keys go when Latch is switched off.
        let mut run = ArpRunner::default();
        run.key_down(&s, 60, 100, 0.0);
        run.key_down(&s, 64, 100, 0.0);
        run.key_up(&s, 60);
        assert_eq!(run.held(), 2);
        run.unlatch();
        assert_eq!(run.held(), 1);
        run.key_up(&ArpSettings { latch: false, ..s }, 64);
        assert_eq!(run.held(), 0);
    }

    #[test]
    fn a_new_chord_restarts_the_pattern_or_joins_the_songs_grid() {
        let mut s = ArpSettings {
            rate: ArpRate::Quarter,
            gate: 50,
            length: 4,
            ..ArpSettings::default()
        };
        s.steps[1].on = false;
        // Restart: each chord plays the pattern from its first step, from where it is pressed.
        let notes = [note(0.0, 3.0, 60), note(3.5, 6.5, 62)];
        let out = render(&s, &notes, 8.0, 1000);
        assert!(close(&starts(&out), &[0.0, 2.0, 3.5, 5.5]), "{out:?}");
        // A key added to a held chord does not restart it.
        let legato = [note(0.0, 4.0, 60), note(2.5, 4.0, 64)];
        assert!(close(
            &starts(&render(&s, &legato, 8.0, 1000)),
            &[0.0, 2.0, 3.0]
        ));
        // Song position: the steps are the song's, so step 1 (beats 1, 5, ...) always rests
        // and a chord pressed between two steps waits for the next.
        s.sync = true;
        let out = render(&s, &notes, 8.0, 1000);
        assert!(close(&starts(&out), &[0.0, 2.0, 4.0, 6.0]), "{out:?}");
        // The keys start from the first one with every chord all the same.
        assert_eq!(pitches(&out), [60, 60, 62, 62]);
        // A key pressed a little late still plays its step, at once.
        let late = render(&s, &[note(2.05, 2.5, 60)], 8.0, 1000);
        assert!(close(&starts(&late), &[2.05]), "{late:?}");
        // Let go and pressed again inside one step: the step is not played twice.
        let twice = render(&s, &[note(2.0, 2.02, 60), note(2.04, 2.5, 60)], 8.0, 1000);
        assert_eq!(twice.len(), 1, "{twice:?}");
    }

    #[test]
    fn the_notes_are_the_same_in_beats_whatever_the_tempo_does() {
        // The generator counts in beats: the engine turns beats into time with the tempo map,
        // so a tempo change in the middle of the pattern moves the notes in time, not in
        // beats. A clock that runs faster from beat 2 gives the same notes as an even one.
        let s = ArpSettings {
            rate: ArpRate::Eighth,
            ..ArpSettings::default()
        };
        let drive = |deltas: &dyn Fn(f64) -> f64| {
            let mut run = ArpRunner::default();
            let mut events = Vec::new();
            let mut beat = 0.0;
            for p in [60, 64, 67] {
                run.key_down(&s, p, 100, beat);
            }
            while beat < 4.0 {
                run.clock(&s, beat, &mut |e| events.push(e));
                beat += deltas(beat);
            }
            events
        };
        let even = drive(&|_| 1.0 / 4096.0);
        let faster = drive(&|b| if b < 2.0 { 1.0 / 4096.0 } else { 1.0 / 1024.0 });
        let ons = |ev: &[ArpEvent]| {
            ev.iter()
                .filter(|e| e.on)
                .map(|e| (e.beat, e.pitch))
                .collect::<Vec<_>>()
        };
        assert_eq!(ons(&even), ons(&faster));
        assert_eq!(ons(&even).len(), 8);
        assert!(ons(&even)
            .iter()
            .enumerate()
            .all(|(k, (b, _))| (b - k as f64 * 0.5).abs() < 1e-9));
    }

    #[test]
    fn a_clock_that_jumps_ends_the_notes_and_starts_the_chord_again() {
        let mut s = quarter(ArpMode::Up);
        s.gate = 100;
        let mut run = ArpRunner::default();
        let mut events: Vec<ArpEvent> = Vec::new();
        for p in [60, 64, 67] {
            run.key_down(&s, p, 100, 10.0);
        }
        let mut beat = 10.0;
        while beat < 11.5 {
            run.clock(&s, beat, &mut |e| events.push(e));
            beat += 1.0 / 1024.0;
        }
        assert_eq!(events.iter().filter(|e| e.on).count(), 2);
        // The song starts from bar 1: the clock goes back to 0.
        events.clear();
        run.clock(&s, 0.0, &mut |e| events.push(e));
        assert_eq!(
            events,
            [
                ArpEvent {
                    beat: 0.0,
                    on: false,
                    pitch: 64,
                    vel: 0
                },
                ArpEvent {
                    beat: 0.0,
                    on: true,
                    pitch: 60,
                    vel: 100
                }
            ]
        );
        // Locked to the song, a chord held over a jump into the middle of a step waits for
        // the next step of the song.
        s.sync = true;
        let mut run = ArpRunner::default();
        run.key_down(&s, 60, 100, 0.0);
        events.clear();
        run.clock(&s, 0.0, &mut |e| events.push(e));
        run.clock(&s, 8.6, &mut |e| events.push(e));
        assert_eq!(events.iter().filter(|e| e.on).count(), 1);
        let mut beat = 8.6;
        while beat < 9.1 {
            run.clock(&s, beat, &mut |e| events.push(e));
            beat += 1.0 / 1024.0;
        }
        let ons: Vec<f64> = events.iter().filter(|e| e.on).map(|e| e.beat).collect();
        assert_eq!(ons, [0.0, 9.0]);
        // Stop ends everything and lets the keys go.
        events.clear();
        run.stop(9.2, &mut |e| events.push(e));
        assert!(events.iter().all(|e| !e.on) && events.len() == 1);
        assert!(run.idle());
    }

    #[test]
    fn every_note_that_starts_ends_and_the_limits_hold() {
        // A busy pattern over many chords: each note on has its note off, none is endless.
        let mut s = ArpSettings {
            rate: ArpRate::ThirtySecondTriplet,
            mode: ArpMode::Chord,
            octaves: 4,
            gate: 200,
            swing: 70,
            ..ArpSettings::default()
        };
        for (i, st) in s.steps.iter_mut().enumerate() {
            st.tie = i % 3 == 0;
            st.chord = i % 2 == 0;
            st.transpose = (i as i8 % 7) * 4 - 12;
        }
        let mut run = ArpRunner::default();
        let mut sounding = [0i32; 128];
        let mut total = 0;
        let mut beat = 0.0;
        let mut k = 0u8;
        while beat < 40.0 {
            if ((beat * 64.0) as u64).is_multiple_of(97) {
                k = k.wrapping_add(7);
                run.key_down(&s, 30 + k % 80, 80, beat);
                run.key_up(&s, 30 + k.wrapping_sub(35) % 80);
            }
            run.clock(&s, beat, &mut |e| {
                sounding[usize::from(e.pitch)] += if e.on { 1 } else { -1 };
                total += 1;
            });
            assert!(sounding.iter().all(|c| (0..=1).contains(c)));
            beat += 1.0 / 512.0;
        }
        assert!(total > 1000);
        run.stop(beat, &mut |e| {
            sounding[usize::from(e.pitch)] -= i32::from(!e.on);
        });
        assert!(sounding.iter().all(|c| *c == 0));
        // More keys than it holds, and more notes than asked for.
        let many: Vec<NoteEvent> = (0..40).map(|i| note(0.0, 100.0, 40 + i)).collect();
        let out = render(&ArpSettings::default(), &many, 100.0, 50);
        assert_eq!(out.len(), 50);
        assert!(out.iter().all(|n| n.pitch < 40 + ARP_KEYS as u8));
        // Wild settings are brought inside their ranges.
        let wild = ArpSettings {
            octaves: 99,
            gate: 0,
            swing: 255,
            step_velocity: 255,
            length: 0,
            ..ArpSettings::default()
        }
        .sanitized();
        assert_eq!(
            (
                wild.octaves,
                wild.gate,
                wild.swing,
                wild.step_velocity,
                wild.length
            ),
            (4, 5, 100, 100, 1)
        );
        // Notes that are not numbers are left out; overlapping ones of a pitch are one key.
        let odd = [
            note(f64::NAN, 1.0, 60),
            note(0.0, 2.0, 62),
            note(1.0, 4.0, 62),
        ];
        let out = render(&quarter(ArpMode::Up), &odd, 4.0, 1000);
        assert_eq!(pitches(&out), [62, 62, 62, 62]);
    }
}
