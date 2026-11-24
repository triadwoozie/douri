// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors. The player is the vendored crate crates/chip-dsp
// (vendor/yamaha-smaf-player: Apache-2.0, Copyright 2026 Akustikrausch (Andreas Wendorf),
// https://github.com/akustikrausch/yamaha-smaf-player at commit 627f7ad), unchanged; see its
// NOTICE.

//! SMAF ringtones (`.mmf`, the format of the FM sound chips in mobile phones around 2004)
//! rendered to audio, for File > Import > SMAF File….
//!
//! Logic Pro equivalent: none.
//!
//! The whole file is rendered at once by the vendored player and its FM core. This is offline
//! work: it allocates and takes as long as the file, and must not run on the audio thread.

/// The longest a rendered file may be, in seconds: a ringtone is seconds long, and a file
/// that says otherwise is cut here.
pub const SMAF_MAX_SECONDS: u32 = 600;

/// Renders the SMAF file in `bytes` at `rate` Hz to interleaved stereo.
pub fn render_smaf(bytes: &[u8], rate: u32) -> Result<Vec<f32>, &'static str> {
    use chip_dsp::SmafError;
    chip_dsp::smaf_render(bytes, rate.max(1), SMAF_MAX_SECONDS).map_err(|e| match e {
        SmafError::NotSmaf => "not a SMAF file",
        SmafError::NothingToPlay => "the file has nothing the player can play",
        SmafError::Failed => "the file could not be rendered",
    })
}

/// A two-note SMAF file made here (no ringtone is kept in the repository): one score track
/// in the older, four-channel format, an A held 400 ms and, half a second on, the E above.
pub fn two_note_smaf() -> Vec<u8> {
    let chunk = |id: &[u8; 4], body: &[u8]| {
        let mut c = id.to_vec();
        c.extend((body.len() as u32).to_be_bytes());
        c.extend(body);
        c
    };
    // Events: a wait, then a note byte (channel, octave, note) and its gate time, both in
    // ticks of 4 ms; three zero bytes end the sequence.
    let sequence = [
        0x00, 0x19, 100, // at 0: octave 1, note 9, 400 ms
        125, 0x24, 100, // 500 ms later: octave 2, note 4, 400 ms
        0x00, 0x00, 0x00, 0x00,
    ];
    // Format 0, a stream sequence, 4 ms ticks for waits and gates, two bytes of channel
    // status.
    let mut track = vec![0x00, 0x00, 0x02, 0x02, 0x00, 0x00];
    track.extend(chunk(b"Mtsq", &sequence));
    let mut body = chunk(b"CNTI", &[0x00, 0x00, 0x00, 0x00, 0x00]);
    body.extend(chunk(b"MTR\x01", &track));
    // The file's check bytes, which the player does not read.
    body.extend([0x00, 0x00]);
    chunk(b"MMMD", &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The file made here renders to its two notes, in time, and silence between them is
    /// quieter than the notes.
    #[test]
    fn a_hand_built_file_renders_its_two_notes() {
        let rate = 44100u32;
        let Ok(audio) = render_smaf(&two_note_smaf(), rate) else {
            panic!("the file does not render")
        };
        assert!(audio.iter().all(|v| v.is_finite()));
        let frames = audio.len() / 2;
        // Both notes and their release: between 0.9 s and the limit.
        assert!(
            frames > rate as usize * 9 / 10 && frames < rate as usize * 30,
            "{frames}"
        );
        let left: Vec<f32> = audio.as_chunks::<2>().0.iter().map(|f| f[0]).collect();
        let peak = |from_ms: usize, to_ms: usize| {
            let (a, b) = (from_ms * rate as usize / 1000, to_ms * rate as usize / 1000);
            left[a..b.min(left.len())]
                .iter()
                .fold(0.0f32, |m, v| m.max(v.abs()))
        };
        let (first, second) = (peak(50, 350), peak(550, 850));
        assert!(first > 0.01 && second > 0.01, "{first} {second}");
        // The second note is higher: more zero crossings in the same time.
        let crossings = |from_ms: usize, to_ms: usize| {
            let (a, b) = (from_ms * rate as usize / 1000, to_ms * rate as usize / 1000);
            left[a..b]
                .windows(2)
                .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
                .count()
        };
        assert!(crossings(550, 850) > crossings(50, 350) * 5 / 4);
    }

    #[test]
    fn what_is_not_a_smaf_file_is_an_error_not_a_crash() {
        assert_eq!(render_smaf(b"", 44100), Err("not a SMAF file"));
        assert_eq!(
            render_smaf(b"RIFF....WAVEfmt ", 44100),
            Err("not a SMAF file")
        );
        // A header with nothing in it, and the real file cut short at every length.
        assert!(render_smaf(b"MMMD\x00\x00\x00\x08CNTI\x00\x00\x00\x00", 44100).is_err());
        let file = two_note_smaf();
        for cut in 0..file.len() {
            let _ = render_smaf(&file[..cut], 8000);
        }
        // Noise after a valid header.
        let mut junk = b"MMMD\x00\x00\x01\x00".to_vec();
        let mut x = 7u32;
        junk.extend((0..256).map(|_| {
            x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (x >> 24) as u8
        }));
        let _ = render_smaf(&junk, 8000);
    }
}
