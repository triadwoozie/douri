//! File > Bounce > Region through EffeTune Chain…: the selected audio region's file run,
//! offline, through a chain of EffeTune effects, and the result put on a new audio track at
//! the region's place.
//!
//! EffeTune (github.com/Frieve-A/effetune, MIT) is not built into Pro Wave: this runs
//! `tools/effetune/apply.py` (kept in the program) with a Python that has the `effetune`
//! package. The Python is the one in the environment
//! variable `PROWAVE_EFFETUNE_PYTHON`, else `effetune-venv/bin/python` in the home folder. The
//! chain is a JSON file the user chooses (the script's own comment says what it holds).
use crate::i18n::{tr, trf};
use crate::model::Project;
use std::path::{Path, PathBuf};
use std::process::Command;

const SCRIPT: &str = include_str!("../../../tools/effetune/apply.py");

/// The command's state: the menu sets `pick`, the next frame opens the file window.
#[derive(Default)]
pub struct EffeTuneUi {
    pub pick: bool,
}

/// The Python that has EffeTune.
pub fn python() -> PathBuf {
    if let Some(p) = std::env::var_os("PROWAVE_EFFETUNE_PYTHON") {
        return PathBuf::from(p);
    }
    let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
    home.join("effetune-venv/bin/python")
}

/// Runs `input` through the chain in `chain` and writes `output` (a 32-bit float WAV of the
/// input's rate, channels and length). The last line of what the script says is the error.
pub fn apply(python: &Path, input: &Path, chain: &Path, output: &Path) -> Result<(), String> {
    let dir = output
        .parent()
        .ok_or_else(|| "the output has no folder".to_string())?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let script = dir.join("effetune-apply.py");
    std::fs::write(&script, SCRIPT).map_err(|e| e.to_string())?;
    let run = Command::new(python)
        .arg(&script)
        .arg(input)
        .arg(output)
        .arg(chain)
        .output();
    let _ = std::fs::remove_file(&script);
    let run = run.map_err(|e| format!("{}: {e}", python.display()))?;
    if run.status.success() && output.is_file() {
        return Ok(());
    }
    let said = String::from_utf8_lossy(&run.stderr);
    Err(said
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("EffeTune did not write the file")
        .trim()
        .to_string())
}

/// A name in `dir` for the processed copy of `source` that no file has yet.
fn free_name(dir: &Path, source: &Path) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Region");
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{stem} EffeTune.wav"))
            } else {
                dir.join(format!("{stem} EffeTune {n}.wav"))
            }
        })
        .find(|p| !p.exists())
        .unwrap_or_else(|| dir.join(format!("{stem} EffeTune.wav")))
}

/// The file and the start bar of the audio region `selected` (track, region).
pub fn region_file(p: &Project, selected: Option<(usize, usize)>) -> Option<(PathBuf, f32)> {
    let (t, r) = selected?;
    let region = p.tracks.get(t)?.regions.get(r)?;
    Some((region.audio.as_ref()?.file.path.clone(), region.start_bar))
}

/// Processes `source` through `chain` into `dir` and adds the result as a new audio track
/// whose region starts at `start_bar`. Returns the file written.
pub fn process(
    p: &mut Project,
    python: &Path,
    source: &Path,
    start_bar: f32,
    chain: &Path,
    dir: &Path,
    ctx: &egui::Context,
) -> Result<PathBuf, String> {
    let out = free_name(dir, source);
    apply(python, source, chain, &out)?;
    crate::import::import_audio(p, &out, ctx).map_err(|e| e.to_string())?;
    if let Some(region) = p.tracks.last_mut().and_then(|t| t.regions.first_mut()) {
        region.start_bar = start_bar;
    }
    Ok(out)
}

/// Runs the command after the menus: the chain's file window, then the processing (the app
/// waits while EffeTune works).
pub fn frame(ctx: &egui::Context, p: &mut Project, s: &mut crate::UiState) {
    if !std::mem::take(&mut s.effetune.pick) {
        return;
    }
    let Some((source, start_bar)) = region_file(p, s.tracks.selected) else {
        s.status = Some(tr("Select an audio region first").to_string());
        return;
    };
    let picked = rfd::FileDialog::new()
        .add_filter("JSON", &["json"])
        .pick_file();
    let Some(chain) = picked else { return };
    let dir = crate::recovery::recordings_in(&crate::recovery::session_dir());
    if let Err(e) = process(p, &python(), &source, start_bar, &chain, &dir, ctx) {
        let shown = source.display();
        s.status = Some(trf("cannot import {}: {}", &[&shown, &e]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A second of a 220 Hz tone, stereo, at 48 kHz.
    fn tone(dir: &Path) -> PathBuf {
        let path = dir.join("tone.wav");
        let stereo: Vec<f32> = (0..48_000)
            .flat_map(|i| {
                let v = (i as f32 * 220.0 * std::f32::consts::TAU / 48_000.0).sin() * 0.5;
                [v, v]
            })
            .collect();
        prowave_engine::write_wav(&path, &stereo, 48_000).unwrap();
        path
    }

    /// Without a Python that runs, the error names it and nothing is added or left behind.
    #[test]
    fn a_missing_python_is_an_error_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut p = Project::empty();
        let before = p.tracks.len();
        let source = tone(dir.path());
        let chain = dir.path().join("chain.json");
        std::fs::write(
            &chain,
            r#"[{"type": "Volume", "parameters": {"volume": -6}}]"#,
        )
        .unwrap();
        let none = dir.path().join("no-such-python");
        let out = dir.path().join("out");
        let e = process(&mut p, &none, &source, 3.0, &chain, &out, &ctx).unwrap_err();
        assert!(e.contains("no-such-python"), "{e}");
        assert_eq!(p.tracks.len(), before);
        assert!(!out.join("tone EffeTune.wav").exists());
        assert!(!out.join("effetune-apply.py").exists());
        // The selection must be an audio region.
        assert!(region_file(&p, None).is_none());
        assert!(region_file(&p, Some((99, 0))).is_none());
        // The script the program carries is the one in tools/.
        assert!(SCRIPT.contains("et.create_effect(") && SCRIPT.contains("et.Chain(effects)"));
    }

    /// With EffeTune installed: the processed copy exists, is as long as the region's file,
    /// is not silent and is quieter by the chain's 6 dB; it lands on a new track at the bar.
    #[test]
    #[ignore = "needs a Python with EffeTune (PROWAVE_EFFETUNE_PYTHON or ~/effetune-venv)"]
    fn a_region_goes_through_a_chain() {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut p = Project::empty();
        let before = p.tracks.len();
        let source = tone(dir.path());
        let chain = dir.path().join("chain.json");
        std::fs::write(
            &chain,
            r#"[{"type": "Volume", "parameters": {"volume": -6}}]"#,
        )
        .unwrap();
        let out = dir.path().join("out");
        let wav = process(&mut p, &python(), &source, 3.0, &chain, &out, &ctx).unwrap();
        assert_eq!(wav, out.join("tone EffeTune.wav"));
        let file = prowave_engine::AudioFile::open(&wav).unwrap();
        assert_eq!(
            (file.frames, file.sample_rate, file.channels),
            (48_000, 48_000, 2)
        );
        let audio = file.read_stereo().unwrap();
        let peak = audio.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        assert!((peak - 0.25).abs() < 0.02, "{peak}");
        assert_eq!(p.tracks.len(), before + 1);
        assert_eq!(p.tracks.last().unwrap().regions[0].start_bar, 3.0);
    }
}
