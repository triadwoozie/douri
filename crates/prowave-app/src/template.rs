//! Project templates (File > New from Template…).

use crate::model::{Project, Track, TrackKind};
use crate::track_icons::IconKind;

/// A project to start from.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Template {
    /// One software instrument track (File > New).
    Empty,
    HipHop,
    Electronic,
    Songwriter,
    Orchestral,
}

impl Template {
    pub const ALL: [Template; 5] = [
        Template::Empty,
        Template::HipHop,
        Template::Electronic,
        Template::Songwriter,
        Template::Orchestral,
    ];

    pub const FACTORY: [Template; 4] = [
        Template::HipHop,
        Template::Electronic,
        Template::Songwriter,
        Template::Orchestral,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Template::Empty => crate::i18n::tr("New Project"),
            Template::HipHop => "Hip Hop",
            Template::Electronic => "Electronic",
            Template::Songwriter => "Songwriter",
            Template::Orchestral => "Orchestral",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Template::Empty => "One software instrument track.",
            Template::HipHop => "808 bass, drum machine and synth lead.",
            Template::Electronic => "Synths, drum machine and a reverb aux.",
            Template::Songwriter => "Piano, drummer, bass and vocal tracks.",
            Template::Orchestral => "Strings, brass, woodwinds and percussion.",
        }
    }

    pub fn icon(self) -> IconKind {
        match self {
            Template::Empty => IconKind::Note,
            Template::HipHop => IconKind::Synth,
            Template::Electronic => IconKind::Synth,
            Template::Songwriter => IconKind::Piano,
            Template::Orchestral => IconKind::Strings,
        }
    }

    /// The new, untitled project this template starts.
    pub fn project(self) -> Project {
        let track = |name: &str, kind, instrument: Option<&'static str>| {
            let mut t = Track::new(name, kind);
            if instrument.is_some() {
                t.instrument = instrument;
            }
            t
        };
        let mut p = Project::empty();
        match self {
            Template::Empty => p,
            Template::HipHop => {
                p.tempo = 140.0;
                p.tracks = vec![
                    track("808 Bass", TrackKind::Instrument, Some("808 Bass")),
                    track("Drum Machine", TrackKind::Drummer, Some("Drum Machine")),
                    track("Sampler Keys", TrackKind::Instrument, Some("Sampler")),
                    track("Lead Synth", TrackKind::Instrument, Some("Vintage B3")),
                ];
                p
            }
            Template::Electronic => {
                p.tempo = 124.0;
                p.tracks = vec![
                    track("Drum Machine", TrackKind::Drummer, Some("Drum Machine")),
                    track("Synth Bass", TrackKind::Instrument, Some("808 Bass")),
                    track("Analog Lead", TrackKind::Instrument, Some("Electric Piano")),
                    track("Reverb", TrackKind::Aux, None),
                ];
                p
            }
            Template::Songwriter => {
                p.tempo = 100.0;
                p.tracks = vec![
                    track("Piano", TrackKind::Instrument, Some("Sampler")),
                    track("Drummer", TrackKind::Drummer, Some("Drummer")),
                    track("Bass", TrackKind::Instrument, Some("808 Bass")),
                    track("Vocal", TrackKind::Audio, None),
                ];
                p
            }
            Template::Orchestral => {
                p.tempo = 90.0;
                p.tracks = vec![
                    track("Strings", TrackKind::Instrument, Some("Sampler")),
                    track("Brass", TrackKind::Instrument, Some("Sampler")),
                    track("Woodwinds", TrackKind::Instrument, Some("Sampler")),
                    track("Timpani", TrackKind::Instrument, Some("Sampler")),
                ];
                p
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_template_builds_a_valid_project() {
        for t in Template::ALL {
            let p = t.project();
            assert!(!p.tracks.is_empty());
        }
    }
}
