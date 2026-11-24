//! File > Import > SMAF File…: a mobile-phone ringtone (`.mmf`) rendered to audio by the
//! engine's `smaf` and put on a new audio track at the playhead, as an imported audio file
//! is. The rendered file is kept with the session's recordings.
use crate::model::Project;
use std::path::{Path, PathBuf};

/// The command's state: the menu sets `pick`, the next frame opens the file window.
#[derive(Default)]
pub struct SmafUi {
    pub pick: bool,
}

/// The rate the ringtone is rendered at.
const RATE: u32 = 48_000;

/// A name in `dir` for the audio of `source` that no file has yet.
fn free_name(dir: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Ringtone");
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{stem}.wav"))
            } else {
                dir.join(format!("{stem} {n}.wav"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{stem}.wav")))
}

/// Renders the SMAF file at `path` into `dir` and adds it to the project as a new audio
/// track. Returns the audio file written.
pub fn import_path(
    p: &mut Project,
    path: &Path,
    dir: &Path,
    ctx: &egui::Context,
) -> Result<PathBuf, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let audio = prowave_engine::smaf::render_smaf(&bytes, RATE)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let wav = free_name(dir, path);
    prowave_engine::write_wav(&wav, &audio, RATE).map_err(|e| e.to_string())?;
    crate::import::import_audio(p, &wav, ctx).map_err(|e| e.to_string())?;
    Ok(wav)
}

/// Runs the command after the menus: the file window, then the import.
pub fn frame(ctx: &egui::Context, p: &mut Project, s: &mut crate::UiState) {
    if !std::mem::take(&mut s.smaf.pick) {
        return;
    }
    let picked = rfd::FileDialog::new()
        .add_filter("SMAF", &["mmf"])
        .pick_file();
    let Some(path) = picked else { return };
    let dir = crate::recovery::recordings_in(&crate::recovery::session_dir());
    if let Err(e) = import_path(p, &path, &dir, ctx) {
        let shown = path.display();
        s.status = Some(crate::i18n::trf("cannot import {}: {}", &[&shown, &e]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ringtone_becomes_an_audio_track_and_a_wrong_file_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut p = Project::empty();
        let before = p.tracks.len();
        let source = dir.path().join("tone.mmf");
        std::fs::write(&source, prowave_engine::smaf::two_note_smaf()).unwrap();
        let out = dir.path().join("media");
        let wav = import_path(&mut p, &source, &out, &ctx).unwrap();
        assert_eq!(wav, out.join("tone.wav"));
        assert_eq!(p.tracks.len(), before + 1);
        let t = p.tracks.last().unwrap();
        assert_eq!(t.kind, crate::model::TrackKind::Audio);
        assert_eq!(t.name, "tone");
        let file = prowave_engine::AudioFile::open(&wav).unwrap();
        let audio = file.read_stereo().unwrap();
        // The two notes take about a second; they are not silent; the output is the file's
        // length, as the same file in the same folder a second time is "tone 2".
        assert!(file.duration_secs() > 0.9, "{}", file.duration_secs());
        assert!(audio.iter().fold(0.0f32, |m, v| m.max(v.abs())) > 0.01);
        assert_eq!(
            import_path(&mut p, &source, &out, &ctx).unwrap(),
            out.join("tone 2.wav")
        );
        // Not a ringtone: an error, no track, no file.
        let wrong = dir.path().join("wrong.mmf");
        std::fs::write(&wrong, b"not a ringtone").unwrap();
        let tracks = p.tracks.len();
        assert_eq!(
            import_path(&mut p, &wrong, &out, &ctx),
            Err("not a SMAF file".into())
        );
        assert_eq!(p.tracks.len(), tracks);
        assert!(!out.join("wrong.wav").exists());
        assert!(import_path(&mut p, &dir.path().join("none.mmf"), &out, &ctx).is_err());
    }
}
