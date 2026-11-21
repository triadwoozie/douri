//! UTAU Project (.ust) Importer and Exporter.
//!
//! Parses and writes classic UTAU `.ust` sequence files, supporting:
//! - Setting blocks (`Tempo`, `Tracks`, `ProjectName`, `VoiceDir`)
//! - Note blocks (`Length`, `Lyric`, `NoteNum`, `PreUtterance`, `VoiceOverlap`, `Intensity`, `Modulation`)
//! - Pitch bends (`PBS`, `PBW`, `PBY`, `PBM`)
//! - Vibrato parameters (`VBR`)

use std::fs::File;
use std::io::Write;
use std::path::Path;

/// Standard UTAU tick resolution: 480 ticks per quarter-note beat.
pub const TICKS_PER_BEAT: u32 = 480;

/// Pitch bend curve definition from `PBS`, `PBW`, `PBY`, `PBM`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UstPitchBend {
    /// Start time offset in ms (from PBS).
    pub start_ms: f64,
    /// Start pitch offset in semitones / cents (from PBS).
    pub start_pitch: f64,
    /// Width of each segment in ms (from PBW).
    pub widths: Vec<f64>,
    /// Pitch height of each point in semitones (from PBY).
    pub heights: Vec<f64>,
    /// Interpolation mode per segment: "s" (spline), "r" (linear), "j", etc.
    pub modes: Vec<String>,
}

/// Vibrato parameters from `VBR`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UstVibrato {
    /// Vibrato length as percentage of note duration (0-100).
    pub length_pct: f64,
    /// Period in ms.
    pub period_ms: f64,
    /// Depth in cents.
    pub depth_cents: f64,
    /// Fade-in time in ms.
    pub fade_in_ms: f64,
    /// Fade-out time in ms.
    pub fade_out_ms: f64,
    /// Initial phase in degrees (0-360).
    pub phase: f64,
    /// Height offset in cents.
    pub height: f64,
}

/// A note in a `.ust` project.
#[derive(Clone, Debug, PartialEq)]
pub struct UstNote {
    /// Position in ticks from the start of the sequence.
    pub position_ticks: u32,
    /// Duration in ticks.
    pub duration_ticks: u32,
    /// MIDI note number (e.g. 60 for C4).
    pub note_num: u8,
    /// Lyric syllable (e.g. "か", "- か", "a", "R" for rest).
    pub lyric: String,
    /// Note velocity (default 100).
    pub velocity: u8,
    /// Note volume / intensity (0-200, default 100).
    pub intensity: u8,
    /// Modulation (0-100, default 0).
    pub modulation: u8,
    /// Preutterance override in ms (optional).
    pub preutterance: Option<f64>,
    /// Voice overlap override in ms (optional).
    pub overlap: Option<f64>,
    /// Pitch bend curve.
    pub pitch_bend: Option<UstPitchBend>,
    /// Vibrato curve.
    pub vibrato: Option<UstVibrato>,
    /// Tempo change at this note (if specified).
    pub tempo: Option<f64>,
}

impl Default for UstNote {
    fn default() -> Self {
        Self {
            position_ticks: 0,
            duration_ticks: TICKS_PER_BEAT,
            note_num: 60,
            lyric: "あ".to_string(),
            velocity: 100,
            intensity: 100,
            modulation: 0,
            preutterance: None,
            overlap: None,
            pitch_bend: None,
            vibrato: None,
            tempo: None,
        }
    }
}

/// A parsed `.ust` project.
#[derive(Clone, Debug, PartialEq)]
pub struct UstProject {
    pub name: String,
    pub tempo: f64,
    pub voice_dir: String,
    pub cache_dir: String,
    pub notes: Vec<UstNote>,
}

impl Default for UstProject {
    fn default() -> Self {
        Self {
            name: "New Project".to_string(),
            tempo: 120.0,
            voice_dir: String::new(),
            cache_dir: String::new(),
            notes: Vec::new(),
        }
    }
}

impl UstProject {
    /// Loads a `.ust` file from disk (handles Shift-JIS or UTF-8).
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let content = crate::voicebank::read_file_detect_encoding(path.as_ref())?;
        Self::parse(&content)
    }

    /// Parses `.ust` INI text content.
    pub fn parse(content: &str) -> std::io::Result<Self> {
        let mut project = UstProject::default();
        let mut current_header = String::new();
        let mut current_lines = Vec::new();
        let mut blocks = Vec::new();

        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                if !current_header.is_empty() {
                    blocks.push((current_header.clone(), std::mem::take(&mut current_lines)));
                }
                current_header = trimmed.to_string();
            } else if !current_header.is_empty() {
                current_lines.push(trimmed.to_string());
            }
        }
        if !current_header.is_empty() {
            blocks.push((current_header, current_lines));
        }

        let mut current_pos_ticks = 0u32;

        for (header, lines) in blocks {
            match header.as_str() {
                "[#SETTING]" => {
                    for line in lines {
                        if let Some((k, v)) = line.split_once('=') {
                            let k = k.trim().to_lowercase();
                            let v = v.trim();
                            match k.as_str() {
                                "tempo" => {
                                    if let Ok(t) = v.parse::<f64>() {
                                        project.tempo = t;
                                    }
                                }
                                "projectname" => project.name = v.to_string(),
                                "voicedir" => project.voice_dir = v.to_string(),
                                "cachedir" => project.cache_dir = v.to_string(),
                                _ => {}
                            }
                        }
                    }
                }
                "[#VERSION]" | "[#TRACKEND]" | "[#PREV]" | "[#NEXT]" => {}
                _ => {
                    // Check if note header [#0000], [#0001], etc.
                    let inner = &header[1..header.len() - 1];
                    let is_note = inner.starts_with('#')
                        && inner[1..].chars().all(|c| c.is_ascii_digit());
                    if is_note {
                        let note = parse_ust_note(&lines, current_pos_ticks);
                        current_pos_ticks += note.duration_ticks;
                        project.notes.push(note);
                    }
                }
            }
        }

        Ok(project)
    }

    /// Serializes project to standard `.ust` text.
    pub fn to_ust_string(&self) -> String {
        let mut out = String::new();
        out.push_str("[#SETTING]\r\n");
        out.push_str(&format!("Tempo={:.2}\r\n", self.tempo));
        out.push_str("Tracks=1\r\n");
        out.push_str(&format!("ProjectName={}\r\n", self.name));
        if !self.voice_dir.is_empty() {
            out.push_str(&format!("VoiceDir={}\r\n", self.voice_dir));
        }
        out.push_str("Mode2=True\r\n");

        for (idx, note) in self.notes.iter().enumerate() {
            out.push_str(&format!("[#{:04}]\r\n", idx));
            out.push_str(&format!("Length={}\r\n", note.duration_ticks));
            out.push_str(&format!("Lyric={}\r\n", note.lyric));
            out.push_str(&format!("NoteNum={}\r\n", note.note_num));
            out.push_str(&format!("Intensity={}\r\n", note.intensity));
            out.push_str(&format!("Modulation={}\r\n", note.modulation));
            out.push_str(&format!("Velocity={}\r\n", note.velocity));

            if let Some(pre) = note.preutterance {
                out.push_str(&format!("PreUtterance={:.1}\r\n", pre));
            }
            if let Some(ovl) = note.overlap {
                out.push_str(&format!("VoiceOverlap={:.1}\r\n", ovl));
            }
            if let Some(t) = note.tempo {
                out.push_str(&format!("Tempo={:.2}\r\n", t));
            }

            if let Some(pb) = &note.pitch_bend {
                out.push_str(&format!("PBS={:.1};{:.1}\r\n", pb.start_ms, pb.start_pitch));
                let w_str = pb
                    .widths
                    .iter()
                    .map(|w| format!("{:.0}", w))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("PBW={}\r\n", w_str));
                let y_str = pb
                    .heights
                    .iter()
                    .map(|h| format!("{:.1}", h))
                    .collect::<Vec<_>>()
                    .join(",");
                out.push_str(&format!("PBY={}\r\n", y_str));
                let m_str = pb.modes.join(",");
                out.push_str(&format!("PBM={}\r\n", m_str));
            }

            if let Some(vbr) = &note.vibrato {
                out.push_str(&format!(
                    "VBR={:.0},{:.1},{:.1},{:.1},{:.1},{:.0},{:.0}\r\n",
                    vbr.length_pct,
                    vbr.period_ms,
                    vbr.depth_cents,
                    vbr.fade_in_ms,
                    vbr.fade_out_ms,
                    vbr.phase,
                    vbr.height
                ));
            }
        }

        out.push_str("[#TRACKEND]\r\n");
        out
    }

    /// Saves project to a `.ust` file (encoded in UTF-8 or Shift-JIS).
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let text = self.to_ust_string();
        let mut file = File::create(path)?;
        file.write_all(text.as_bytes())?;
        Ok(())
    }
}

fn parse_ust_note(lines: &[String], pos_ticks: u32) -> UstNote {
    let mut note = UstNote {
        position_ticks: pos_ticks,
        ..Default::default()
    };

    let mut pbs_str = None;
    let mut pbw_str = None;
    let mut pby_str = None;
    let mut pbm_str = None;

    for line in lines {
        if let Some((k, v)) = line.split_once('=') {
            let k = k.trim().to_lowercase();
            let v = v.trim();
            match k.as_str() {
                "length" => {
                    if let Ok(len) = v.parse::<u32>() {
                        note.duration_ticks = len;
                    }
                }
                "lyric" => note.lyric = v.to_string(),
                "notenum" => {
                    if let Ok(num) = v.parse::<u8>() {
                        note.note_num = num;
                    }
                }
                "velocity" => {
                    if let Ok(vel) = v.parse::<u8>() {
                        note.velocity = vel;
                    }
                }
                "intensity" => {
                    if let Ok(vol) = v.parse::<u8>() {
                        note.intensity = vol;
                    }
                }
                "modulation" => {
                    if let Ok(m) = v.parse::<u8>() {
                        note.modulation = m;
                    }
                }
                "preutterance" => {
                    note.preutterance = v.parse::<f64>().ok();
                }
                "voiceoverlap" => {
                    note.overlap = v.parse::<f64>().ok();
                }
                "tempo" => {
                    note.tempo = v.parse::<f64>().ok();
                }
                "pbs" => pbs_str = Some(v.to_string()),
                "pbw" => pbw_str = Some(v.to_string()),
                "pby" => pby_str = Some(v.to_string()),
                "pbm" => pbm_str = Some(v.to_string()),
                "vbr" => {
                    let parts: Vec<f64> = v.split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if parts.len() >= 3 {
                        note.vibrato = Some(UstVibrato {
                            length_pct: parts.get(0).copied().unwrap_or(100.0),
                            period_ms: parts.get(1).copied().unwrap_or(100.0),
                            depth_cents: parts.get(2).copied().unwrap_or(50.0),
                            fade_in_ms: parts.get(3).copied().unwrap_or(0.0),
                            fade_out_ms: parts.get(4).copied().unwrap_or(0.0),
                            phase: parts.get(5).copied().unwrap_or(0.0),
                            height: parts.get(6).copied().unwrap_or(0.0),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    if pbs_str.is_some() || pbw_str.is_some() || pby_str.is_some() {
        let mut pb = UstPitchBend::default();
        if let Some(pbs) = pbs_str {
            if let Some((t, p)) = pbs.split_once(';') {
                pb.start_ms = t.trim().parse().unwrap_or(0.0);
                pb.start_pitch = p.trim().parse().unwrap_or(0.0);
            } else {
                pb.start_ms = pbs.trim().parse().unwrap_or(0.0);
            }
        }
        if let Some(pbw) = pbw_str {
            pb.widths = pbw
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
        }
        if let Some(pby) = pby_str {
            pb.heights = pby
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
        }
        if let Some(pbm) = pbm_str {
            pb.modes = pbm.split(',').map(|s| s.trim().to_string()).collect();
        }
        note.pitch_bend = Some(pb);
    }

    note
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_writes_ust() {
        let ust_raw = r#"
[#SETTING]
Tempo=135.00
Tracks=1
ProjectName=Test Track
VoiceDir=TestSinger
Mode2=True

[#0000]
Length=480
Lyric=か
NoteNum=60
Intensity=100
Modulation=0
Velocity=100
PBS=-20.0;0.0
PBW=50,50
PBY=0.0,1.5
PBM=s,s
VBR=65,120.0,40.0,20.0,20.0,0,0

[#0001]
Length=960
Lyric=わ
NoteNum=62
Intensity=100
Modulation=0
Velocity=100

[#TRACKEND]
"#;

        let proj = UstProject::parse(ust_raw).expect("parses cleanly");
        assert_eq!(proj.tempo, 135.0);
        assert_eq!(proj.name, "Test Track");
        assert_eq!(proj.notes.len(), 2);

        assert_eq!(proj.notes[0].position_ticks, 0);
        assert_eq!(proj.notes[0].duration_ticks, 480);
        assert_eq!(proj.notes[0].lyric, "か");
        assert_eq!(proj.notes[0].note_num, 60);

        let pb = proj.notes[0].pitch_bend.as_ref().unwrap();
        assert_eq!(pb.start_ms, -20.0);
        assert_eq!(pb.widths, vec![50.0, 50.0]);
        assert_eq!(pb.heights, vec![0.0, 1.5]);

        let vbr = proj.notes[0].vibrato.as_ref().unwrap();
        assert_eq!(vbr.length_pct, 65.0);
        assert_eq!(vbr.period_ms, 120.0);
        assert_eq!(vbr.depth_cents, 40.0);

        assert_eq!(proj.notes[1].position_ticks, 480);
        assert_eq!(proj.notes[1].duration_ticks, 960);
        assert_eq!(proj.notes[1].lyric, "わ");

        // Round-trip test
        let emitted = proj.to_ust_string();
        let proj2 = UstProject::parse(&emitted).expect("re-parses emitted ust cleanly");
        assert_eq!(proj2.notes.len(), 2);
        assert_eq!(proj2.notes[0].lyric, "か");
        assert_eq!(proj2.notes[1].lyric, "わ");
    }
}
