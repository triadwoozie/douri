//! Bounce in Place and the Join that makes a new file (Logic Pro: File > Bounce > Regions in
//! Place, Tracks in Place; Edit > Join on audio regions that are not pieces of one file).
//!
//! Each track's part is played by the bounce's own offline engine (`bounce.rs`): the project
//! cut down to that track, those regions and the track's own strip (`bounce::Strip`), over the
//! regions' time span. Nothing here renders sound another way. When every part is written,
//! the project is changed in one go (one undo step): a new audio track under each source
//! track with the new region at the same bar (or the regions replaced on their own track).
//!
//! What follows Logic, and where it differs:
//! - Destination New Track (the default) or Replace. Replace puts the file where the regions
//!   were, on the same strip, so the strip's effects, fader and pan are left out of it (they
//!   would be heard twice); it is offered only when every region is on an audio track.
//! - Source (New Track): Leave, Mute or Delete the regions; for Tracks in Place, Leave or Mute
//!   the track.
//! - Include Audio Tail (on by default): the file goes on while the effects still ring.
//! - Volume/Pan Automation: Copy (Logic's default) leaves the fader, pan, their automation and
//!   the VCA out of the file and gives them to the new track; Include puts them in the file
//!   and the new track is at 0 dB, centre.
//! - Effects: On, or Bypass (Logic: Bypass Effect Plug-ins). The instrument always plays.
//! - Normalize: Off or On.
//! - One file per track (Logic's "One File" per track); Logic can also make a file per region.
//! - Sends and the output to a bus are not in the file: the new track takes the source's sends
//!   and output and plays through them.
//! - Join renders the regions as they play on their track without its strip (region gain,
//!   fades, mute), into one region from the first one's start to the last one's end.
//! - Convert Regions to New Audio Files (Logic: Option-Command-F) renders each selected audio
//!   region by itself as it plays without its strip (region gain, fades, reverse, Flex, its
//!   loop repeats) into a file of its own, named after the region, and the new region takes
//!   its place: the same name, colour, mute and crossfade, and nothing left for the region's
//!   own settings to do. Logic's dialog (sample rate, bit depth, file type, dither) is not
//!   offered: the files are 32-bit float WAV at the project's rate, as Bounce in Place's.
use crate::bounce::{self, Depth, FileReport, Format, Job, Progress, Report, Settings, Strip};
use crate::model::{AudioRegion, Project, Region, RegionParams, Track, TrackKind};
use crate::plugins::PluginHost;
use crate::tracks_area::TracksView;
use std::path::{Path, PathBuf};

/// What is bounced.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum What {
    /// File > Bounce > Regions in Place… (Control-B).
    Regions,
    /// File > Bounce > Tracks in Place… (Control-Command-B): every region of the selected
    /// track.
    Tracks,
    /// Edit > Join (Command-J) on audio regions that are not consecutive pieces of one file.
    Join,
    /// Edit > Convert > Convert Regions to New Audio Files (Option-Command-F): each selected
    /// audio region of an audio track, one file each, in place.
    Convert,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Destination {
    NewTrack,
    Replace,
}

/// What happens to what was bounced (with Destination New Track).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Leave,
    Mute,
    /// Regions only.
    Delete,
}

/// The dialog's settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    pub destination: Destination,
    pub source: Source,
    pub tail: bool,
    /// Include Volume/Pan Automation: the fader and pan are in the file.
    pub automation: bool,
    /// Bypass Effect Plug-ins.
    pub bypass: bool,
    pub normalize: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            destination: Destination::NewTrack,
            source: Source::Leave,
            tail: true,
            automation: false,
            bypass: false,
            normalize: false,
        }
    }
}

/// A bounce asked for: what, how, and the regions of each track (taken when it was asked).
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub what: What,
    pub options: Options,
    pub pieces: Vec<(usize, Vec<usize>)>,
}

/// Track kinds whose regions are bounced: those that play by themselves in the app.
fn bounceable(kind: TrackKind) -> bool {
    matches!(
        kind,
        TrackKind::Audio | TrackKind::Instrument | TrackKind::Drummer
    )
}

/// An audio region that plays its own file as it is (not a take folder or a missing file).
fn plain_audio(r: &Region) -> bool {
    r.audio.is_some() && r.folder.is_none() && r.generated.is_none() && r.missing.is_none()
}

/// The regions `what` takes from the selection (`sel`) or the selected track: per track, its
/// region indexes in order. Empty when there is nothing to bounce.
pub fn pieces_of(p: &Project, what: What, sel: &[(usize, usize)]) -> Vec<(usize, Vec<usize>)> {
    let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
    match what {
        What::Tracks => {
            let t = p.selected_track;
            if let Some(track) = p.tracks.get(t) {
                if bounceable(track.kind) && !track.regions.is_empty() {
                    out.push((t, (0..track.regions.len()).collect()));
                }
            }
        }
        What::Convert => {
            let mut sel = sel.to_vec();
            sel.sort_unstable();
            sel.dedup();
            for (t, r) in sel {
                let ok = p.tracks.get(t).is_some_and(|track| {
                    track.kind == TrackKind::Audio && track.regions.get(r).is_some_and(plain_audio)
                });
                if ok {
                    out.push((t, vec![r]));
                }
            }
        }
        What::Regions | What::Join => {
            for &(t, r) in sel {
                let Some(track) = p.tracks.get(t) else {
                    continue;
                };
                if r >= track.regions.len() || !bounceable(track.kind) {
                    continue;
                }
                match out.iter_mut().find(|(x, _)| *x == t) {
                    Some((_, rs)) => rs.push(r),
                    None => out.push((t, vec![r])),
                }
            }
            for (_, rs) in &mut out {
                rs.sort_unstable();
                rs.dedup();
            }
            if what == What::Join {
                // Two or more audio regions of an audio track that Join cannot put together
                // as pieces of one file.
                out.retain(|(t, rs)| {
                    let track = &p.tracks[*t];
                    track.kind == TrackKind::Audio
                        && rs.len() >= 2
                        && rs.iter().all(|&r| plain_audio(&track.regions[r]))
                        && !p.can_join_group(*t, rs)
                });
            }
        }
    }
    out
}

/// Whether Replace can be offered: every track bounced is an audio track.
pub fn can_replace(p: &Project, pieces: &[(usize, Vec<usize>)]) -> bool {
    !pieces.is_empty()
        && pieces
            .iter()
            .all(|(t, _)| p.tracks.get(*t).is_some_and(|x| x.kind == TrackKind::Audio))
}

/// What a region looked like when the bounce was asked for: the project must still have it
/// so when the result is put in.
#[derive(Clone, Debug, PartialEq)]
struct Mark {
    name: String,
    start_bar: f32,
    len_bars: f32,
    loop_beats: f32,
}

impl Mark {
    fn of(r: &Region) -> Self {
        Self {
            name: r.name.clone(),
            start_bar: r.start_bar,
            len_bars: r.len_bars,
            loop_beats: r.params.loop_beats,
        }
    }
}

/// One track's part.
#[derive(Clone, Debug)]
struct Piece {
    track: usize,
    track_name: String,
    regions: Vec<usize>,
    marks: Vec<Mark>,
    /// The span in quarter notes, and the bar it starts at.
    from: f64,
    to: f64,
    start_bar: f32,
    path: PathBuf,
}

/// A file name in `dir` that is not taken: "Bass_bip.wav", "Bass_bip 2.wav"…
fn free_path(dir: &Path, stem: &str, taken: &[PathBuf]) -> PathBuf {
    let mut n = 1;
    loop {
        let name = if n == 1 {
            format!("{stem}.wav")
        } else {
            format!("{stem} {n}.wav")
        };
        let path = dir.join(name);
        if !path.exists() && !taken.contains(&path) {
            return path;
        }
        n += 1;
    }
}

/// What a finished run did.
#[derive(Clone, Debug, PartialEq)]
pub struct Done {
    /// The regions made, as (track, region) in the project as it is now.
    pub made: Vec<(usize, usize)>,
    /// The bounce's report: the files and what could not be in them.
    pub report: Report,
}

/// A Bounce in Place or Join running: one bounce job per track, one after the other. Made by
/// [`Run::start`]; [`Run::poll`] every frame until it gives the result.
pub struct Run {
    what: What,
    options: Options,
    pieces: Vec<Piece>,
    at: usize,
    job: Option<Job>,
    rate: u32,
    files: Vec<FileReport>,
    not_rendered: Vec<String>,
}

impl Run {
    /// Checks the request against `p` and starts the first track's bounce. The files go in
    /// `dir` (the session's recordings folder: they are saved into the project with it).
    pub fn start(p: &Project, req: &Request, rate: u32, dir: &Path) -> Result<Self, String> {
        if req.pieces.is_empty() {
            return Err("nothing to bounce: no region of a track that plays".into());
        }
        if req.options.destination == Destination::Replace
            && !matches!(req.what, What::Join | What::Convert)
            && (req.what == What::Tracks || !can_replace(p, &req.pieces))
        {
            return Err("Replace works on audio tracks only".into());
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut pieces: Vec<Piece> = Vec::new();
        let mut taken = Vec::new();
        for (t, rs) in &req.pieces {
            let track = p.tracks.get(*t).ok_or("the track is not there any more")?;
            if rs.is_empty() || rs.iter().any(|&r| r >= track.regions.len()) {
                return Err("the regions are not there any more".into());
            }
            let regions: Vec<&Region> = rs.iter().map(|&r| &track.regions[r]).collect();
            let from = regions
                .iter()
                .map(|r| p.region_start_beats(r))
                .fold(f64::MAX, f64::min);
            let to = regions
                .iter()
                .map(|r| p.region_loop_end_beats(r))
                .fold(f64::MIN, f64::max);
            let start_bar = regions.iter().map(|r| r.start_bar).fold(f32::MAX, f32::min);
            if to <= from {
                return Err("the regions have no length".into());
            }
            let stem = match req.what {
                What::Convert => bounce::file_safe(if regions[0].name.trim().is_empty() {
                    &track.name
                } else {
                    &regions[0].name
                }),
                What::Join => format!("{}_join", bounce::file_safe(&track.name)),
                _ => format!("{}_bip", bounce::file_safe(&track.name)),
            };
            let path = free_path(dir, &stem, &taken);
            taken.push(path.clone());
            pieces.push(Piece {
                track: *t,
                track_name: track.name.clone(),
                regions: rs.clone(),
                marks: regions.iter().map(|r| Mark::of(r)).collect(),
                from,
                to,
                start_bar,
                path,
            });
        }
        let mut run = Self {
            what: req.what,
            options: req.options.clone(),
            pieces,
            at: 0,
            job: None,
            rate,
            files: Vec::new(),
            not_rendered: Vec::new(),
        };
        run.job = Some(Job::start(p, rate, run.settings(0))?);
        Ok(run)
    }

    /// Whether the strip's effects and fader are in the file.
    fn heard_strip(&self) -> (bool, bool) {
        let o = &self.options;
        let region_level = self.region_level();
        if region_level {
            (false, false)
        } else {
            (!o.bypass, o.automation)
        }
    }

    /// Whether the regions are rendered as they play on their track, without its strip, and
    /// the file takes their place.
    fn region_level(&self) -> bool {
        matches!(self.what, What::Join | What::Convert)
            || self.options.destination == Destination::Replace
    }

    /// What is bounced.
    pub fn what(&self) -> What {
        self.what
    }

    /// The bounce of piece `i`.
    fn settings(&self, i: usize) -> Settings {
        let piece = &self.pieces[i];
        let (effects, fader) = self.heard_strip();
        let o = &self.options;
        let tail = o.tail && !matches!(self.what, What::Join | What::Convert);
        Settings {
            path: piece.path.clone(),
            range: Some((piece.from, piece.to)),
            format: Format::Wav,
            // Float, so the file is what the engine played, to the bit.
            depth: Depth::Float32,
            normalize: o.normalize && !matches!(self.what, What::Join | What::Convert),
            tail_seconds: 0.0,
            max_tail_seconds: if tail { 20.0 } else { 0.0 },
            mix: true,
            stems: false,
            only: Some(piece.track),
            overwrite: false,
            strip: Some(Strip {
                track: piece.track,
                regions: piece.regions.clone(),
                effects,
                fader,
                mute_automation: !matches!(self.what, What::Join | What::Convert),
                unmute_regions: self.what == What::Convert,
            }),
        }
    }

    /// How many regions are bounced (all tracks together), for the status line.
    pub fn regions_bounced(&self) -> usize {
        self.pieces.iter().map(|x| x.regions.len()).sum()
    }

    pub fn cancel(&self) {
        if let Some(job) = &self.job {
            job.cancel();
        }
    }

    pub fn progress(&self) -> Progress {
        let n = self.pieces.len().max(1);
        let (within, preparing) = self
            .job
            .as_ref()
            .map(|j| {
                let pr = j.progress();
                (pr.fraction, pr.preparing)
            })
            .unwrap_or((1.0, false));
        Progress {
            fraction: ((self.at as f32 + within) / n as f32).clamp(0.0, 1.0),
            file: (self.at + 1).min(n),
            files: n,
            track: self.pieces.get(self.at).map(|x| x.track_name.clone()),
            preparing,
        }
    }

    /// The files written so far are removed.
    fn discard(&self) {
        for f in &self.files {
            let _ = std::fs::remove_file(&f.path);
        }
    }

    /// One step, every frame (see `bounce::Job::poll` for `busy`). When it is over, the result
    /// has been put into `live` (and the new regions selected in `view`), or nothing changed.
    /// `ctx` repaints the waveforms once they are read.
    pub fn poll(
        &mut self,
        live: &mut Project,
        plugins: &mut PluginHost,
        busy: bool,
        ctx: Option<&egui::Context>,
        view: &mut TracksView,
    ) -> Option<Result<Done, String>> {
        let job = self.job.as_mut()?;
        let result = job.poll(live, plugins, busy)?;
        self.job = None;
        let report = match result {
            Ok(r) => r,
            Err(e) => {
                self.discard();
                return Some(Err(e));
            }
        };
        self.files.extend(report.files.iter().cloned());
        for f in report.not_rendered {
            if !self.not_rendered.contains(&f) {
                self.not_rendered.push(f);
            }
        }
        self.at += 1;
        if self.at < self.pieces.len() {
            match Job::start(live, self.rate, self.settings(self.at)) {
                Ok(job) => {
                    self.job = Some(job);
                    return None;
                }
                Err(e) => {
                    self.discard();
                    return Some(Err(e));
                }
            }
        }
        let result = self.put_in(live, ctx, view);
        if result.is_err() {
            self.discard();
        }
        Some(result)
    }

    /// Puts the files into the project, all in one go.
    fn put_in(
        &self,
        p: &mut Project,
        ctx: Option<&egui::Context>,
        view: &mut TracksView,
    ) -> Result<Done, String> {
        // The project may have changed while the files were made: the regions must still be
        // where they were.
        for piece in &self.pieces {
            let same = p.tracks.get(piece.track).is_some_and(|t| {
                t.name == piece.track_name
                    && piece
                        .regions
                        .iter()
                        .zip(&piece.marks)
                        .all(|(&r, m)| t.regions.get(r).is_some_and(|x| Mark::of(x) == *m))
            });
            if !same {
                return Err(
                    "the regions changed while they were bounced: nothing was changed".into(),
                );
            }
        }
        let mut files = Vec::new();
        for f in &self.files {
            files.push(prowave_engine::AudioFile::open(&f.path).map_err(|e| e.to_string())?);
        }
        let map = p.tempo_map();
        // From the last track up, so the tracks put in do not move those still to come.
        let mut order: Vec<usize> = (0..self.pieces.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(self.pieces[i].track));
        let mut made: Vec<(usize, usize)> = Vec::new();
        for i in order {
            let piece = &self.pieces[i];
            let file = files[i].clone();
            let end = map.advance(piece.from, file.duration_secs());
            let len_bars = (p.beats_to_bar(end) - (f64::from(piece.start_bar) - 1.0)) as f32;
            let first = &p.tracks[piece.track].regions[piece.regions[0]];
            let name = match self.what {
                What::Join | What::Convert => first.name.clone(),
                _ => format!("{}_bip", piece.track_name),
            };
            // Convert: what the file does not hold stays with the region.
            let params = match self.what {
                What::Convert => RegionParams {
                    color: first.params.color,
                    mute: first.params.mute,
                    crossfade: first.params.crossfade,
                    fade_curve: first.params.fade_curve,
                    ..RegionParams::default()
                },
                _ => RegionParams::default(),
            };
            let region = Region {
                missing: None,
                name,
                start_bar: piece.start_bar,
                len_bars,
                notes: Vec::new(),
                controls: Vec::new(),
                folder: None,
                audio: Some(audio_of(file, ctx)),
                params,
                generated: None,
            };
            let replace = self.region_level();
            if replace {
                let at = replace_regions(p, piece.track, &piece.regions, region);
                made.push((piece.track, at));
                continue;
            }
            let t = piece.track;
            let mut new = new_track_for(&p.tracks[t], &self.options);
            new.regions.push(region);
            let vca = new.vca;
            p.tracks.insert(t + 1, new);
            p.vca_inserted(t + 1);
            p.tracks[t + 1].vca = vca;
            if p.selected_track > t {
                p.selected_track += 1;
            }
            // What was made on the tracks below moves down by one.
            for m in made.iter_mut().filter(|m| m.0 > t) {
                m.0 += 1;
            }
            match (self.what, self.options.source) {
                (_, Source::Leave) => {}
                (What::Tracks, _) => p.tracks[t].mute = true,
                (_, Source::Mute) => {
                    for &r in &piece.regions {
                        p.tracks[t].regions[r].params.mute = true;
                    }
                }
                (_, Source::Delete) => {
                    for &r in piece.regions.iter().rev() {
                        p.tracks[t].regions.remove(r);
                    }
                }
            }
            made.push((t + 1, 0));
        }
        p.song_rev += 1;
        made.sort_unstable();
        view.select_many(&made);
        Ok(Done {
            made,
            report: Report {
                files: self.files.clone(),
                not_rendered: self.not_rendered.clone(),
                sample_rate: self.rate,
                format: format!("{}, {}", Format::Wav.name(), Depth::Float32.name()),
                ..Report::default()
            },
        })
    }
}

/// The region's file, with its waveform read in the background when there is a window.
fn audio_of(file: prowave_engine::AudioFile, ctx: Option<&egui::Context>) -> AudioRegion {
    match ctx {
        Some(ctx) => crate::import::audio_region(file, ctx),
        None => AudioRegion {
            file,
            peaks: Default::default(),
            flex: None,
        },
    }
}

/// Regions `rs` (in order) of track `t` give way to `region`, which takes the place of the
/// first; returns its index.
fn replace_regions(p: &mut Project, t: usize, rs: &[usize], region: Region) -> usize {
    let regions = &mut p.tracks[t].regions;
    for &r in rs.iter().rev() {
        regions.remove(r);
    }
    let at = rs[0].min(regions.len());
    regions.insert(at, region);
    at
}

/// The audio track a bounce of `src` goes on: its name, colour, place in a stack, output and
/// sends; with Volume/Pan Automation copied, its fader, pan, their automation and its VCA too.
fn new_track_for(src: &Track, o: &Options) -> Track {
    let mut t = Track::new(format!("{}_bip", src.name), TrackKind::Audio);
    t.color = src.color;
    t.indent = src.indent;
    t.output = src.output;
    t.sends = src.sends.clone();
    t.auto.sends = src.auto.sends.clone();
    t.height = src.height;
    if !o.automation {
        t.volume_db = src.volume_db;
        t.pan = src.pan;
        t.vca = src.vca;
        t.auto.volume = src.auto.volume.clone();
        t.auto.pan = src.auto.pan.clone();
    }
    t
}

/// The Bounce in Place dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct Dialog {
    pub what: What,
    pub options: Options,
}

/// Opens the dialog for `what` (Regions or Tracks), with the settings it had last.
pub fn open(s: &mut crate::UiState, what: What) {
    let b = &mut s.bounce;
    if b.job.is_some() || b.in_place.is_some() {
        return;
    }
    let mut options = b.in_place_remembered.clone().unwrap_or_default();
    if what == What::Tracks {
        options.destination = Destination::NewTrack;
        if options.source == Source::Delete {
            options.source = Source::Leave;
        }
    }
    b.in_place_dialog = Some(Dialog { what, options });
}

/// Whether `what` has something to bounce now and nothing else is bouncing.
pub fn can(p: &Project, s: &crate::UiState, what: What) -> bool {
    s.bounce.job.is_none()
        && s.bounce.in_place.is_none()
        && !pieces_of(p, what, &s.tracks.selection()).is_empty()
}

/// Edit > Join's part: the audio regions it cannot join as pieces of one file are bounced into
/// a new one. Called before the regions that can be joined are.
pub fn ask_join(p: &Project, s: &mut crate::UiState) {
    if s.bounce.job.is_some() || s.bounce.in_place.is_some() {
        return;
    }
    let pieces = pieces_of(p, What::Join, &s.tracks.selection());
    if !pieces.is_empty() {
        s.bounce.in_place_go = Some(Request {
            what: What::Join,
            options: Options::default(),
            pieces,
        });
    }
}

/// Edit > Convert > Convert Regions to New Audio Files: asks for the conversion of the
/// selected audio regions.
pub fn ask_convert(p: &Project, s: &mut crate::UiState) {
    if s.bounce.job.is_some() || s.bounce.in_place.is_some() {
        return;
    }
    let pieces = pieces_of(p, What::Convert, &s.tracks.selection());
    if !pieces.is_empty() {
        s.bounce.in_place_go = Some(Request {
            what: What::Convert,
            options: Options::default(),
            pieces,
        });
    }
}

/// The dialog, drawn while it is open. OK asks for the bounce (`BounceUi::in_place_go`).
pub fn show_dialog(ctx: &egui::Context, p: &Project, s: &mut crate::UiState) {
    use crate::hw::Cap;
    use crate::i18n::tr;
    use crate::spectrogram_view::hw_vocal;
    let Some(mut d) = s.bounce.in_place_dialog.clone() else {
        return;
    };
    let pieces = pieces_of(p, d.what, &s.tracks.selection());
    let replace_ok = d.what == What::Regions && can_replace(p, &pieces);
    if !replace_ok {
        d.options.destination = Destination::NewTrack;
    }
    let id = egui::Id::unique("prowave_bounce_in_place");
    let subtitle = match d.what {
        What::Tracks => tr("Tracks in Place…"),
        _ => tr("Regions in Place…"),
    };
    let mut go = false;
    let mut cancel = false;
    let (open, _) = hw_vocal::window(ctx, id, tr("Bounce"), subtitle, 380.0, None, |ui| {
        let o = &mut d.options;
        let dest = [tr("New Track"), tr("Replace")];
        let items: &[&str] = if replace_ok { &dest } else { &dest[..1] };
        let at = usize::from(o.destination == Destination::Replace);
        let field = hw_vocal::row(ui, tr("Destination"));
        if let Some(i) = hw_vocal::dropdown(ui, id.with("dest"), field, dest[at], items, Some(at)) {
            o.destination = if i == 1 {
                Destination::Replace
            } else {
                Destination::NewTrack
            };
        }
        let on_off = [tr("Off"), tr("On")];
        if o.destination == Destination::NewTrack {
            let sources = [tr("Leave"), tr("Mute"), tr("Delete")];
            let items: &[&str] = if d.what == What::Tracks {
                &sources[..2]
            } else {
                &sources
            };
            let at = match o.source {
                Source::Leave => 0,
                Source::Mute => 1,
                Source::Delete => 2,
            };
            let field = hw_vocal::row(ui, tr("Source"));
            if let Some(i) =
                hw_vocal::dropdown(ui, id.with("source"), field, sources[at], items, Some(at))
            {
                o.source = [Source::Leave, Source::Mute, Source::Delete][i];
            }
            let fx = [tr("On"), tr("Bypass")];
            let at = usize::from(o.bypass);
            let field = hw_vocal::row(ui, tr("Effects"));
            if let Some(i) = hw_vocal::dropdown(ui, id.with("fx"), field, fx[at], &fx, Some(at)) {
                o.bypass = i == 1;
            }
            let auto = [tr("Copy"), tr("Include")];
            let at = usize::from(o.automation);
            let field = hw_vocal::row(ui, tr("Volume/Pan Automation"));
            if let Some(i) =
                hw_vocal::dropdown(ui, id.with("auto"), field, auto[at], &auto, Some(at))
            {
                o.automation = i == 1;
            }
        }
        let at = usize::from(o.tail);
        let field = hw_vocal::row(ui, tr("Tail"));
        if let Some(i) =
            hw_vocal::dropdown(ui, id.with("tail"), field, on_off[at], &on_off, Some(at))
        {
            o.tail = i == 1;
        }
        let at = usize::from(o.normalize);
        let field = hw_vocal::row(ui, tr("Normalize"));
        if let Some(i) =
            hw_vocal::dropdown(ui, id.with("norm"), field, on_off[at], &on_off, Some(at))
        {
            o.normalize = i == 1;
        }
        hw_vocal::gap(ui, 12.0);
        let clicked = hw_vocal::key_row(
            ui,
            id.with("keys"),
            &[
                (tr("Cancel"), Cap::Bone, None, true),
                (tr("OK"), Cap::Orange, None, !pieces.is_empty()),
            ],
        );
        cancel = clicked[0];
        go = clicked[1];
    });
    let b = &mut s.bounce;
    if d.what == What::Regions {
        b.in_place_remembered = Some(d.options.clone());
    }
    if go && !pieces.is_empty() {
        b.in_place_go = Some(Request {
            what: d.what,
            options: d.options,
            pieces,
        });
        b.in_place_dialog = None;
    } else if cancel || !open {
        b.in_place_dialog = None;
    } else {
        b.in_place_dialog = Some(d);
    }
}

#[cfg(test)]
#[path = "bounce_in_place_tests.rs"]
mod tests;
