//! Import audio files as new audio tracks (Logic Pro: File > Import > Audio File, or drag-and-drop).
use crate::model::{AudioRegion, Project, Region, RegionParams, Track, TrackKind, TrackParams};
use prowave_engine::{AudioError, AudioFile};
use std::path::Path;
use std::sync::{Arc, OnceLock};

const PEAK_BINS: usize = 4096;

pub fn is_audio_path(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "wav"
                | "wave"
                | "aif"
                | "aiff"
                | "aifc"
                | "flac"
                | "mp3"
                | "caf"
                | "ogg"
                | "oga"
                | "m4a"
                | "mp4"
        )
    })
}

/// Wraps an audio file as a region source and computes its waveform overview on a
/// background thread (the region shows a flat line until it is ready).
pub fn audio_region(file: AudioFile, ctx: &egui::Context) -> AudioRegion {
    let peaks = Arc::new(OnceLock::new());
    {
        let (file, peaks, ctx) = (file.clone(), peaks.clone(), ctx.clone());
        std::thread::spawn(move || match file.peaks(PEAK_BINS) {
            Ok(v) => {
                let _ = peaks.set(v);
                ctx.request_repaint();
            }
            Err(e) => tracing::warn!("waveform overview failed for {}: {e}", file.path.display()),
        });
    }
    AudioRegion::whole(file, peaks)
}

/// Adds a new audio track with one region starting at the bar under the playhead.
pub fn import_audio(p: &mut Project, path: &Path, ctx: &egui::Context) -> Result<(), AudioError> {
    import_audio_at(p, path, None, None, ctx)
}

/// Imports an audio file onto `track_idx` (if it is an existing Audio track) at `beat`, or
/// creates a new Audio track at `beat` (defaulting to the playhead bar).
pub fn import_audio_at(
    p: &mut Project,
    path: &Path,
    track_idx: Option<usize>,
    beat: Option<f64>,
    ctx: &egui::Context,
) -> Result<(), AudioError> {
    let file = AudioFile::open(path)?;
    let anchor_beats = beat.unwrap_or(p.position_beats).max(0.0);
    let start_bar = if beat.is_some() {
        p.beats_to_bar(anchor_beats) + 1.0
    } else {
        p.beats_to_bar(anchor_beats).floor() + 1.0
    };
    // The region's length follows the Tempo track from where it starts.
    let map = p.tempo_map();
    let start = p.bar_to_beats(start_bar - 1.0);
    let end = map.advance(start, file.duration_secs());
    let len_bars = p.beats_to_bar(end) - (start_bar - 1.0);
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Audio")
        .to_string();

    let region = Region {
        name: name.clone(),
        start_bar: start_bar as f32,
        len_bars: len_bars as f32,
        notes: vec![],
        controls: Vec::new(),
        folder: None,
        generated: None,
        audio: Some(audio_region(file, ctx)),
        missing: None,
        params: RegionParams::default(),
    };

    if let Some(ti) = track_idx {
        if let Some(track) = p.tracks.get_mut(ti) {
            if track.kind == TrackKind::Audio {
                track.regions.push(region);
                p.selected_track = ti;
                p.song_rev += 1;
                return Ok(());
            }
        }
    }

    p.tracks.push(Track {
        name,
        kind: TrackKind::Audio,
        color: egui::Color32::from_rgb(0x3f, 0x5f, 0xc0),
        mute: false,
        solo: false,
        rec: false,
        monitor: false,
        input: prowave_engine::InputSource::Mono(0),
        output: 0,
        aux_bus: 0,
        volume_db: 0.0,
        pan: 0.0,
        inserts: vec![],
        instrument: None,
        regions: vec![region],
        params: TrackParams::default(),
        category: None,
        sends: Vec::new(),
        auto: crate::model::Automation::default(),
        group: None,
        height: 1.0,
        indent: 0,
        folded: false,
        hidden: false,
        vca: None,
        midi_port: None,
        soundfont: None,
        sampler: None,
        epiano: None,
        drums: None,
        bass808: None,
        marbble_inst: None,
        instrument_bypass: false,
        hosted_instrument: 0,
        icon: None,
        midi_in_port: None,
        midi_in_channel: None,
        midi_channel: 1,
        midi_program: None,
        midi_bank: None,
        midi_fx: Vec::new(),
        au_instrument: None,
        au_effects: Vec::new(),
        summing_stack: false,
    });
    p.selected_track = p.tracks.len() - 1;
    p.song_rev += 1;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn import_audio_at_places_region_on_audio_track_at_target_beat() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("kick.wav");
        let samples = vec![0.0f32; 4_410];
        prowave_engine::stems::write_wav_stereo(&wav, &samples, &samples, 44_100).unwrap();

        let mut p = Project::empty();
        p.tracks.push(Track::new("Audio 1", TrackKind::Audio));
        let audio_idx = p.tracks.len() - 1;
        let ctx = egui::Context::default();

        import_audio_at(&mut p, &wav, Some(audio_idx), Some(8.0), &ctx).unwrap();
        assert_eq!(p.tracks[audio_idx].regions.len(), 1);
        assert!((p.tracks[audio_idx].regions[0].start_bar - 3.0).abs() < 1e-4);
    }
}
