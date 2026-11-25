// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! Voice allocation for the native instruments: which of `N` voices plays a note.
//!
//! Logic Pro equivalent: an instrument's Voices setting and its voice stealing.

/// Which of `N` voices holds which note. A new note takes a voice that is idle, or else the
/// one that began longest ago (voices already let go first).
#[derive(Clone, Copy, Debug)]
pub struct Voices<const N: usize> {
    /// The note a voice plays while its key is down.
    held: [Option<u8>; N],
    /// The voice still sounds (held, or ringing out).
    busy: [bool; N],
    started: [u64; N],
    clock: u64,
}

impl<const N: usize> Default for Voices<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Voices<N> {
    pub const fn new() -> Self {
        Voices {
            held: [None; N],
            busy: [false; N],
            started: [0; N],
            clock: 0,
        }
    }

    /// The voice for a new `note` among the first `limit` voices. A voice that still holds
    /// the same note is used again.
    pub fn start(&mut self, note: u8, limit: usize) -> usize {
        let limit = limit.clamp(1, N.max(1)).min(N);
        self.clock += 1;
        let pick = (0..limit)
            .find(|&i| self.held[i] == Some(note))
            .or_else(|| (0..limit).find(|&i| !self.busy[i]))
            .or_else(|| {
                (0..limit)
                    .filter(|&i| self.held[i].is_none())
                    .min_by_key(|&i| self.started[i])
            })
            .or_else(|| (0..limit).min_by_key(|&i| self.started[i]))
            .unwrap_or(0);
        if let Some(h) = self.held.get_mut(pick) {
            *h = Some(note);
        }
        if let Some(b) = self.busy.get_mut(pick) {
            *b = true;
        }
        if let Some(s) = self.started.get_mut(pick) {
            *s = self.clock;
        }
        pick
    }

    /// The voice that held `note`, now let go (it stays busy until [`Voices::idle`]).
    pub fn stop(&mut self, note: u8) -> Option<usize> {
        let i = self.held.iter().position(|h| *h == Some(note))?;
        self.held[i] = None;
        Some(i)
    }

    /// Lets every held voice go, calling `each` with its index.
    pub fn stop_all(&mut self, mut each: impl FnMut(usize)) {
        for (i, h) in self.held.iter_mut().enumerate() {
            if h.take().is_some() {
                each(i);
            }
        }
    }

    /// Voice `i` has finished sounding.
    pub fn idle(&mut self, i: usize) {
        if let (Some(b), Some(h)) = (self.busy.get_mut(i), self.held.get(i)) {
            if h.is_none() {
                *b = false;
            }
        }
    }

    /// Voice `i` has finished sounding though its key may still be down (a one-shot).
    pub fn release_voice(&mut self, i: usize) {
        if let Some(h) = self.held.get_mut(i) {
            *h = None;
        }
        if let Some(b) = self.busy.get_mut(i) {
            *b = false;
        }
    }

    /// Every voice idle and no note held.
    pub fn clear(&mut self) {
        self.held = [None; N];
        self.busy = [false; N];
    }

    pub fn is_held(&self, i: usize) -> bool {
        self.held.get(i).is_some_and(|h| h.is_some())
    }

    pub fn is_busy(&self, i: usize) -> bool {
        self.busy.get(i).copied().unwrap_or(false)
    }

    /// The voice begun most recently that is still held.
    pub fn newest_held(&self) -> Option<usize> {
        (0..N)
            .filter(|&i| self.held[i].is_some())
            .max_by_key(|&i| self.started[i])
    }

    pub fn note(&self, i: usize) -> Option<u8> {
        self.held.get(i).copied().flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_voices_first_then_released_then_the_oldest() {
        let mut v = Voices::<3>::new();
        assert_eq!(v.start(60, 3), 0);
        assert_eq!(v.start(62, 3), 1);
        assert_eq!(v.start(64, 3), 2);
        // The same note again takes its own voice.
        assert_eq!(v.start(62, 3), 1);
        // All held: the oldest goes.
        assert_eq!(v.start(65, 3), 0);
        // A released voice goes before a held one, though it is newer.
        assert_eq!(v.stop(65), Some(0));
        assert_eq!(v.start(67, 3), 0);
        assert_eq!(v.stop(64), Some(2));
        v.idle(2);
        assert!(!v.is_busy(2) && v.is_busy(1));
        assert_eq!(v.stop(99), None);
        // One voice only: every note takes it.
        let mut m = Voices::<3>::new();
        assert_eq!(m.start(60, 1), 0);
        assert_eq!(m.start(61, 1), 0);
        assert_eq!(m.stop(60), None);
        assert_eq!(m.newest_held(), Some(0));
        let mut let_go = 0;
        m.stop_all(|_| let_go += 1);
        assert_eq!(let_go, 1);
        // A held voice is not made idle, but a one-shot that is over frees it.
        m.start(60, 1);
        m.idle(0);
        assert!(m.is_busy(0));
        m.release_voice(0);
        assert!(!m.is_busy(0) && !m.is_held(0) && m.stop(60).is_none());
    }
}
