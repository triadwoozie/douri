//! OpenUtau Project (.ustx) Importer and Exporter.
//!
//! Parses and writes modern OpenUtau YAML `.ustx` project files.

use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

/// Pitch bend control point in a `.ustx` note.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UstxPitchPoint {
    pub x: f64,
    pub y: f64,
    #[serde(default)]
    pub shape: Option<String>,
}

/// Pitch curve definition for a note in `.ustx`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct UstxPitch {
    #[serde(default)]
    pub data: Vec<UstxPitchPoint>,
    #[serde(default)]
    pub snap_first: Option<bool>,
}

/// Vibrato parameters for a note in `.ustx`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct UstxVibrato {
    pub length: f64,
    pub period: f64,
    pub depth: f64,
    #[serde(rename = "in", default)]
    pub fade_in: f64,
    #[serde(default)]
    pub out: f64,
    #[serde(default)]
    pub shift: f64,
    #[serde(default)]
    pub drift: f64,
}

/// A note in a `.ustx` voice part.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UstxNote {
    pub position: i32,
    pub duration: i32,
    pub tone: i32,
    pub lyric: String,
    #[serde(default)]
    pub pitch: Option<UstxPitch>,
    #[serde(default)]
    pub vibrato: Option<UstxVibrato>,
}

/// A voice part in a `.ustx` project.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct UstxVoicePart {
    pub name: String,
    pub comment: Option<String>,
    #[serde(rename = "track_no")]
    pub track_no: usize,
    pub position: i32,
    #[serde(default)]
    pub notes: Vec<UstxNote>,
}

/// A track in a `.ustx` project.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct UstxTrack {
    #[serde(rename = "track_name", default)]
    pub track_name: Option<String>,
    #[serde(rename = "track_color", default)]
    pub track_color: Option<String>,
    #[serde(rename = "voice_color_names", default)]
    pub voice_color_names: Vec<String>,
    #[serde(rename = "singer", default)]
    pub singer: Option<String>,
    #[serde(rename = "phonemizer", default)]
    pub phonemizer: Option<String>,
    #[serde(rename = "renderer_settings", default)]
    pub renderer_settings: Option<serde_yaml::Value>,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default)]
    pub volume: Option<f64>,
    #[serde(default)]
    pub pan: Option<f64>,
}

/// Tempo marker in a `.ustx` project.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UstxTempo {
    pub position: i32,
    pub bpm: f64,
}

/// Time signature marker in a `.ustx` project.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UstxTimeSignature {
    #[serde(rename = "bar_position")]
    pub bar_position: i32,
    #[serde(rename = "beat_per_bar")]
    pub beat_per_bar: i32,
    #[serde(rename = "beat_unit")]
    pub beat_unit: i32,
}

/// An OpenUtau project file (.ustx).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UstxProject {
    #[serde(rename = "ustx_version", default = "default_ustx_version")]
    pub ustx_version: String,
    pub name: String,
    #[serde(default)]
    pub comment: String,
    #[serde(rename = "output_dir", default = "default_output_dir")]
    pub output_dir: String,
    #[serde(rename = "cache_dir", default = "default_cache_dir")]
    pub cache_dir: String,
    #[serde(rename = "time_signatures", default)]
    pub time_signatures: Vec<UstxTimeSignature>,
    #[serde(default)]
    pub tempos: Vec<UstxTempo>,
    #[serde(default)]
    pub tracks: Vec<UstxTrack>,
    #[serde(rename = "voice_parts", default)]
    pub voice_parts: Vec<UstxVoicePart>,
}

fn default_ustx_version() -> String {
    "0.6".to_string()
}
fn default_output_dir() -> String {
    "Vocal".to_string()
}
fn default_cache_dir() -> String {
    "UCache".to_string()
}

impl Default for UstxProject {
    fn default() -> Self {
        Self {
            ustx_version: default_ustx_version(),
            name: "New Project".to_string(),
            comment: String::new(),
            output_dir: default_output_dir(),
            cache_dir: default_cache_dir(),
            time_signatures: vec![UstxTimeSignature {
                bar_position: 0,
                beat_per_bar: 4,
                beat_unit: 4,
            }],
            tempos: vec![UstxTempo {
                position: 0,
                bpm: 120.0,
            }],
            tracks: vec![UstxTrack {
                track_name: Some("Track 1".to_string()),
                ..Default::default()
            }],
            voice_parts: Vec::new(),
        }
    }
}

impl UstxProject {
    /// Loads a `.ustx` YAML file from disk.
    pub fn load(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let mut file = File::open(path)?;
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        Self::parse(&content)
    }

    /// Parses `.ustx` YAML content.
    pub fn parse(yaml: &str) -> std::io::Result<Self> {
        serde_yaml::from_str(yaml)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Serializes project to `.ustx` YAML string.
    pub fn to_ustx_string(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Saves project to `.ustx` file on disk.
    pub fn save(&self, path: impl AsRef<Path>) -> std::io::Result<()> {
        let yaml = self
            .to_ustx_string()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut file = File::create(path)?;
        file.write_all(yaml.as_bytes())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_serializes_ustx() {
        let yaml = r#"
ustx_version: "0.6"
name: "Ustx Test"
comment: "Testing YAML"
output_dir: "Vocal"
cache_dir: "UCache"
time_signatures:
  - bar_position: 0
    beat_per_bar: 4
    beat_unit: 4
tempos:
  - position: 0
    bpm: 128.0
tracks:
  - track_name: "Lead Vocal"
    singer: "Defoko"
voice_parts:
  - name: "Chorus"
    track_no: 0
    position: 0
    notes:
      - position: 0
        duration: 480
        tone: 60
        lyric: "か"
        pitch:
          data:
            - { x: -40.0, y: 0.0, shape: "io" }
            - { x: 40.0, y: 0.0, shape: "io" }
      - position: 480
        duration: 480
        tone: 62
        lyric: "わ"
"#;

        let proj = UstxProject::parse(yaml).expect("parses cleanly");
        assert_eq!(proj.name, "Ustx Test");
        assert_eq!(proj.tempos[0].bpm, 128.0);
        assert_eq!(proj.tracks.len(), 1);
        assert_eq!(proj.voice_parts.len(), 1);
        let part = &proj.voice_parts[0];
        assert_eq!(part.notes.len(), 2);
        assert_eq!(part.notes[0].lyric, "か");
        assert_eq!(part.notes[0].tone, 60);

        let roundtrip = proj.to_ustx_string().expect("serializes cleanly");
        let proj2 = UstxProject::parse(&roundtrip).expect("re-parses cleanly");
        assert_eq!(proj2.name, "Ustx Test");
        assert_eq!(proj2.voice_parts[0].notes.len(), 2);
    }
}
