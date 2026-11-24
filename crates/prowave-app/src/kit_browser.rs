//! The Kit Browser (the Kits key of the Channel Rack): every drum kit of the library folders,
//! searched as it is typed, by source, the ranked ones, the favourites; the samples of the kit
//! picked, with what each one is; and the rack's first sixteen channels as pads.
//!
//! Load Kit puts the kit on the rack's channels ([`crate::kit_files::Kit::pads`]): the steps
//! stay, so a pattern plays the new kit. A sample goes on one channel by a double-click (the
//! pad that is lit) or by dragging it onto a pad here or onto a channel of the rack. Nothing
//! sounds until its key is pressed: the triangle of a sample plays that sample, Audition the
//! kit's first channels one after the other, both on the engine's audition voice.
//!
//! The index is made on a background thread ([`crate::kit_library`]) and a kit's files are
//! read on another when it is picked, so neither the window nor the audio waits for a disk. A
//! library folder that is not there (a drive that is not mounted) shows an unlit lamp in
//! Folders and nothing else.
use crate::channel_rack::{self, SampleDrag};
use crate::gear::{self, LedAt};
use crate::hw::{self, Cap};
use crate::i18n::tr;
use crate::kit_classify::{DrumClass, KIT_PADS};
use crate::kit_files::{self, Kit};
use crate::kit_library::{self, Filter, Index, ScanMsg, Settings};
use crate::midi_monitor::hw_utilities as hu;
use crate::model::Project;
use crate::theme::{self, Face};
use egui::{pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Ui};
use prowave_engine::{AuditionMsg, AuditionRemote, SampleBuf, AUDITION_LAYERS};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Duration;

pub const WIDTH: f32 = 920.0;
const PAD: f32 = 18.0;
const ROW: f32 = 24.0;
const LIST_H: f32 = 336.0;
const SOURCES_W: f32 = 196.0;
const KITS_W: f32 = 330.0;
const GAP: f32 = 10.0;
/// Silence after a sound on the audition voice (which loops): time to stop it in.
const TAIL_SECONDS: f64 = 0.6;
/// The longest a sample is played for.
const MAX_AUDITION_SECONDS: f64 = 8.0;
/// A kit's audition: this many of its channels, one every `KIT_STEP_SECONDS`.
const KIT_HITS: usize = 8;
const KIT_STEP_SECONDS: f64 = 0.3;
const KIT_HIT_SECONDS: f64 = 0.9;

/// What is on the audition voice.
struct Playing {
    key: String,
    /// The phase of the loop where the sound ends and the silence starts.
    end: f32,
    last: f32,
    heard: bool,
}

/// A sound read for the audition: the buffer (sound and silence) and where the sound ends.
type Ready = Result<(SampleBuf, f32), String>;

#[derive(Default)]
pub struct KitBrowser {
    pub open: bool,
    started: bool,
    /// Where the settings and the cache are kept (`None`: nowhere, as in tests).
    data: Option<PathBuf>,
    pub settings: Settings,
    pub index: Index,
    pub scanning: bool,
    /// How long the last scan of the folders took.
    pub last_scan: Option<Duration>,
    scan_rx: Option<Receiver<ScanMsg>>,
    pub filter: Filter,
    /// Bumped when the index or the favourites change.
    rev: u64,
    results: Vec<u32>,
    results_for: Option<(Filter, u64)>,
    /// The kit picked (its folder).
    pub selected: Option<String>,
    kit: Option<Arc<Kit>>,
    kit_rx: Option<(String, Receiver<Kit>)>,
    kit_stop: Option<Arc<AtomicBool>>,
    class: Option<DrumClass>,
    /// The rack channel a double-clicked sample goes on.
    pub pad: usize,
    pub show_folders: bool,
    scroll_to: Option<usize>,
    want: Option<(String, Receiver<Ready>)>,
    send: Option<(String, Arc<SampleBuf>, f32)>,
    playing: Option<Playing>,
    stop: bool,
    pub error: Option<String>,
}

impl KitBrowser {
    /// A browser that keeps its settings and cache in `data` and looks in `folders`.
    #[cfg(test)]
    pub fn with(data: Option<PathBuf>, folders: Vec<String>) -> Self {
        let mut v = Self {
            data,
            started: true,
            ..Self::default()
        };
        v.settings.folders = folders;
        v
    }

    /// Reads the settings and starts the first scan, once.
    pub fn start(&mut self) {
        if self.started {
            return;
        }
        self.started = true;
        if cfg!(test) {
            // A test looks only where it says: never in the library of the machine it runs on.
            self.settings.folders.clear();
        } else {
            self.data = kit_library::data_dir();
        }
        if let Some(dir) = &self.data {
            self.settings = Settings::load(dir);
        }
        self.rescan();
    }

    /// Scans the folders again in the background.
    pub fn rescan(&mut self) {
        self.scanning = true;
        self.scan_rx = Some(kit_library::spawn_scan(
            self.settings.folders.clone(),
            self.data.clone(),
        ));
    }

    /// Scans the folders now, on this thread (the control interface, which waits for it).
    pub fn scan_now(&mut self) -> Duration {
        self.start();
        self.scan_rx = None;
        let cache = self
            .data
            .as_deref()
            .map(kit_library::Cache::load)
            .unwrap_or_default();
        let started = std::time::Instant::now();
        let (index, cache) = kit_library::scan(&self.settings.folders, cache);
        let took = started.elapsed();
        if let Some(dir) = &self.data {
            if let Err(e) = cache.save(dir) {
                tracing::warn!("could not write the kit index: {e}");
            }
        }
        self.take_index(index);
        self.scanning = false;
        self.last_scan = Some(took);
        took
    }

    fn take_index(&mut self, index: Index) {
        self.index = index;
        self.rev += 1;
        if self
            .filter
            .source
            .as_ref()
            .is_some_and(|s| !self.index.sources.iter().any(|(n, _)| n == s))
        {
            self.filter.source = None;
        }
    }

    /// Takes what the background threads have done.
    pub fn poll(&mut self) {
        if let Some(rx) = self.scan_rx.take() {
            let mut done = false;
            loop {
                match rx.try_recv() {
                    Ok(ScanMsg::Cached(index)) => {
                        if self.index.kits.is_empty() {
                            self.take_index(index);
                        }
                    }
                    Ok(ScanMsg::Scanned(index, took)) => {
                        self.take_index(index);
                        self.last_scan = Some(took);
                        // Nothing anywhere: the folders are what there is to look at.
                        if self.index.kits.is_empty() {
                            self.show_folders = true;
                        }
                        done = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
            if done {
                self.scanning = false;
            } else {
                self.scan_rx = Some(rx);
            }
        }
        if let Some((path, rx)) = self.kit_rx.take() {
            match rx.try_recv() {
                Ok(kit) => self.take_kit(&path, kit),
                Err(TryRecvError::Empty) => self.kit_rx = Some((path, rx)),
                Err(TryRecvError::Disconnected) => {}
            }
        }
    }

    fn take_kit(&mut self, path: &str, kit: Kit) {
        // The index learns how many audio files the kit has.
        let files = kit.samples.len() as u32;
        if let Some(k) = self.index.kits.iter_mut().find(|k| k.path == path) {
            if k.files != Some(files) {
                k.files = Some(files);
                if let Some(dir) = self.data.clone() {
                    let (root, path) = (k.root.clone(), path.to_string());
                    let _ = std::thread::Builder::new()
                        .name("kit-count".into())
                        .spawn(move || kit_library::note_count(&dir, &root, &path, files));
                }
            }
        }
        if self.selected.as_deref() == Some(path) {
            self.kit = Some(Arc::new(kit));
            self.class = None;
        }
    }

    /// Picks the kit in folder `path` and reads its files in the background.
    pub fn select(&mut self, path: &str) {
        if self.selected.as_deref() == Some(path) {
            return;
        }
        self.selected = Some(path.to_string());
        self.kit = None;
        // The read of the kit picked before is given up.
        if let Some(stop) = self.kit_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        let root = PathBuf::from(path);
        let flag = stop.clone();
        let spawned = std::thread::Builder::new()
            .name("kit-read".into())
            .spawn(move || {
                let kit = kit_files::read_kit_until(&root, &flag);
                if !flag.load(Ordering::Relaxed) {
                    let _ = tx.send(kit);
                }
            });
        if spawned.is_ok() {
            self.kit_rx = Some((path.to_string(), rx));
            self.kit_stop = Some(stop);
        }
    }

    /// Picks the kit and reads it now, on this thread (the control interface).
    pub fn select_now(&mut self, path: &str) -> Arc<Kit> {
        if let Some(stop) = self.kit_stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        self.selected = Some(path.to_string());
        self.kit_rx = None;
        let kit = kit_files::read_kit(Path::new(path));
        self.take_kit(path, kit.clone());
        self.kit.clone().unwrap_or_else(|| Arc::new(kit))
    }

    /// The kit picked, once its files are read.
    pub fn kit(&self) -> Option<&Arc<Kit>> {
        self.kit.as_ref()
    }

    /// The kits the filter keeps, as indices into the index.
    pub fn results(&mut self) -> &[u32] {
        let key = (self.filter.clone(), self.rev);
        if self.results_for.as_ref() != Some(&key) {
            self.results = kit_library::search(&self.index, &self.filter, &self.settings.favorites);
            self.results_for = Some(key);
        }
        &self.results
    }

    fn save_settings(&self) {
        if let Some(dir) = &self.data {
            if let Err(e) = self.settings.save(dir) {
                tracing::warn!("could not save the kit library settings: {e}");
            }
        }
    }

    /// Sets the library folders, saves them and scans.
    pub fn set_folders(&mut self, folders: Vec<String>) {
        self.start();
        let mut seen = std::collections::HashSet::new();
        self.settings.folders = folders
            .into_iter()
            .filter(|f| !f.trim().is_empty() && seen.insert(f.clone()))
            .collect();
        self.save_settings();
        self.rescan();
    }

    pub fn set_favorite(&mut self, path: &str, on: bool) {
        let changed = if on {
            self.settings.favorites.insert(path.to_string())
        } else {
            self.settings.favorites.remove(path)
        };
        if changed {
            self.rev += 1;
            self.save_settings();
        }
    }

    pub fn is_favorite(&self, path: &str) -> bool {
        self.settings.favorites.contains(path)
    }

    /// The name of the kit picked, from the index.
    fn selected_name(&self) -> String {
        let Some(path) = &self.selected else {
            return String::new();
        };
        self.index
            .kits
            .iter()
            .find(|k| &k.path == path)
            .map(|k| k.name.clone())
            .unwrap_or_else(|| {
                Path::new(path)
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default()
            })
    }

    fn audition(&mut self, key: String, paths: Vec<PathBuf>, kit: bool) {
        // The same key again stops it.
        if self.playing.as_ref().is_some_and(|p| p.key == key)
            || self.want.as_ref().is_some_and(|w| w.0 == key)
        {
            self.stop_audition();
            return;
        }
        let (tx, rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("kit-audition".into())
            .spawn(move || {
                let _ = tx.send(if kit {
                    kit_preview(&paths)
                } else {
                    paths
                        .first()
                        .ok_or_else(|| "nothing to play".to_string())
                        .and_then(|p| sample_preview(p))
                });
            });
        if spawned.is_ok() {
            self.want = Some((key, rx));
        }
    }

    pub fn stop_audition(&mut self) {
        self.want = None;
        self.send = None;
        if self.playing.is_some() {
            self.stop = true;
        }
    }

    fn audition_key(&self) -> Option<&str> {
        self.playing
            .as_ref()
            .map(|p| p.key.as_str())
            .or(self.send.as_ref().map(|s| s.0.as_str()))
            .or(self.want.as_ref().map(|w| w.0.as_str()))
    }

    fn busy(&self) -> bool {
        self.scanning
            || self.kit_rx.is_some()
            || self.want.is_some()
            || self.send.is_some()
            || self.playing.is_some()
            || self.stop
    }
}

/// The channels a kit load fills, as files.
pub fn pad_paths(kit: &Kit) -> Vec<Option<PathBuf>> {
    kit.pads.iter().map(|p| p.map(|i| kit.path_of(i))).collect()
}

/// Loads the kit picked into the rack of the selected track (a new rack track when the
/// selected track has none). Returns the track.
pub fn load_selected(v: &mut KitBrowser, p: &mut Project) -> Option<usize> {
    let kit = v.kit.clone()?;
    if kit.pads.is_empty() {
        return None;
    }
    let name = v.selected_name();
    Some(channel_rack::load_kit(
        p,
        channel_rack::rack_track(p),
        &name,
        &pad_paths(&kit),
    ))
}

/// Puts `path` on channel `channel` of the selected track's rack (a new rack track when the
/// selected track has none).
pub fn assign(p: &mut Project, channel: usize, path: &Path) -> bool {
    let ti = match channel_rack::rack_track(p) {
        Some(i) => i,
        None => channel_rack::load_kit(p, None, "", &[]),
    };
    channel_rack::assign_sample(p, ti, channel, path)
}

// ------------------------------------------------------------------------------- audition

/// `buf` cut to `seconds`, faded out at the cut, with the silence the voice stops in after it.
fn with_tail(mut buf: SampleBuf, seconds: f64) -> (SampleBuf, f32) {
    let rate = f64::from(buf.rate.max(1));
    let keep = ((seconds * rate) as usize).min(buf.frames());
    if keep < buf.frames() {
        buf.data.truncate(keep * 2);
        let fade = ((0.02 * rate) as usize).min(keep);
        for i in 0..fade {
            let g = i as f32 / fade as f32;
            let f = keep - 1 - i;
            buf.data[f * 2] *= g;
            buf.data[f * 2 + 1] *= g;
        }
    }
    let sound = buf.frames().max(1);
    let tail = (TAIL_SECONDS * rate) as usize;
    buf.data.resize((sound + tail) * 2, 0.0);
    let end = sound as f32 / (sound + tail) as f32;
    (buf, end)
}

fn sample_preview(path: &Path) -> Ready {
    let buf = prowave_engine::sample_tools::decode(path, |_| true)?;
    if buf.frames() < 2 {
        return Err(format!("{}: empty", path.display()));
    }
    Ok(with_tail(buf, MAX_AUDITION_SECONDS))
}

/// The first samples of a kit's channels, one after the other.
fn kit_preview(paths: &[PathBuf]) -> Ready {
    let mut out: Option<SampleBuf> = None;
    let mut hit = 0usize;
    for path in paths.iter().take(KIT_HITS) {
        let Ok(buf) = prowave_engine::sample_tools::decode(path, |_| true) else {
            continue;
        };
        if buf.frames() < 2 {
            continue;
        }
        let mix = out.get_or_insert_with(|| SampleBuf::new(buf.rate, Vec::new()));
        let rate = f64::from(mix.rate.max(1));
        let start = (hit as f64 * KIT_STEP_SECONDS * rate) as usize;
        let ratio = f64::from(buf.rate.max(1)) / rate;
        let frames =
            ((buf.frames() as f64 / ratio) as usize).min((KIT_HIT_SECONDS * rate) as usize);
        if mix.data.len() < (start + frames) * 2 {
            mix.data.resize((start + frames) * 2, 0.0);
        }
        let fade = ((0.03 * rate) as usize).min(frames);
        for i in 0..frames {
            let pos = i as f64 * ratio;
            let a = (pos as usize).min(buf.frames() - 1);
            let b = (a + 1).min(buf.frames() - 1);
            let t = (pos - a as f64) as f32;
            let g = if frames - i <= fade {
                (frames - i) as f32 / fade as f32
            } else {
                1.0
            };
            for c in 0..2 {
                let s = buf.data[a * 2 + c] + (buf.data[b * 2 + c] - buf.data[a * 2 + c]) * t;
                mix.data[(start + i) * 2 + c] += s * g;
            }
        }
        hit += 1;
    }
    let mix = out.ok_or_else(|| "nothing to play".to_string())?;
    Ok(with_tail(mix, f64::MAX))
}

/// Tells the audition voice what to play and stops it when the sound has ended (the voice
/// loops; the silence after the sound is the time there is to stop it).
pub fn pump(v: &mut KitBrowser, remote: Option<&mut AuditionRemote>) {
    if let Some((key, rx)) = v.want.take() {
        match rx.try_recv() {
            Ok(Ok((buf, end))) => v.send = Some((key, Arc::new(buf), end)),
            Ok(Err(e)) => v.error = Some(e),
            Err(TryRecvError::Empty) => v.want = Some((key, rx)),
            Err(TryRecvError::Disconnected) => {}
        }
    }
    let Some(remote) = remote else {
        // No engine: nothing plays.
        v.send = None;
        v.playing = None;
        v.stop = false;
        return;
    };
    if v.stop {
        if remote.send(AuditionMsg::Stop).is_ok() {
            v.stop = false;
            v.playing = None;
        }
        return;
    }
    if let Some((key, buf, end)) = v.send.take() {
        let mut layers: prowave_engine::AuditionLayers = Default::default();
        layers[0] = Some(buf);
        let mut gains = [0.0f32; AUDITION_LAYERS];
        gains[0] = 1.0;
        let _ = remote.send(AuditionMsg::Sync(false));
        let _ = remote.send(AuditionMsg::Volume(1.0));
        let sent = remote.send(AuditionMsg::Play {
            layers,
            gains,
            beats: 4.0,
            from_start: true,
        });
        if sent.is_ok() {
            v.playing = Some(Playing {
                key,
                end,
                last: 0.0,
                heard: false,
            });
        }
        return;
    }
    if let Some(p) = &mut v.playing {
        match remote.status.phase() {
            Some(phase) => {
                let over = phase >= p.end || (p.heard && phase + 0.05 < p.last);
                p.heard = true;
                p.last = phase;
                if over {
                    v.stop = true;
                }
            }
            None if p.heard => v.playing = None,
            None => {}
        }
    }
    remote.collect();
}

// ------------------------------------------------------------------------------- drawing

fn star_points(c: Pos2, r: f32) -> Vec<Pos2> {
    (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let rr = if i % 2 == 0 { r } else { r * 0.44 };
            pos2(c.x + a.cos() * rr, c.y + a.sin() * rr)
        })
        .collect()
}

/// A star: filled when `on`, an outline otherwise.
fn star(g: &egui::Painter, c: Pos2, r: f32, on: bool, color: Color32) {
    let pts = star_points(c, r);
    if on {
        for i in 0..5 {
            let tip = pts[i * 2];
            let a = pts[(i * 2 + 9) % 10];
            let b = pts[(i * 2 + 1) % 10];
            g.add(egui::Shape::convex_polygon(
                vec![tip, b, c, a],
                color,
                Stroke::NONE,
            ));
        }
    } else {
        g.add(egui::Shape::closed_line(pts, Stroke::new(1.2, color)));
    }
}

/// The triangle of a sample (a square while it plays).
fn play_glyph(g: &egui::Painter, c: Pos2, playing: bool, color: Color32) {
    if playing {
        g.rect_filled(Rect::from_center_size(c, vec2(8.0, 8.0)), 1.0, color);
    } else {
        g.add(egui::Shape::convex_polygon(
            vec![
                pos2(c.x - 3.5, c.y - 5.0),
                pos2(c.x + 5.0, c.y),
                pos2(c.x - 3.5, c.y + 5.0),
            ],
            color,
            Stroke::NONE,
        ));
    }
}

/// The floor of a list, with its name printed over it.
fn floor(g: &egui::Painter, rect: Rect, label: &str) -> Rect {
    hw::silk(
        g,
        pos2(rect.left() + 2.0, rect.top() + 6.0),
        Align2::LEFT_CENTER,
        label,
        10.5,
        theme::INK2,
    );
    let r = Rect::from_min_max(pos2(rect.left(), rect.top() + 16.0), rect.max);
    g.rect_filled(r, 3.0, theme::BONE2);
    hw::inner_shadow(g, r, 3.0, 0.18);
    g.rect_stroke(
        r,
        3.0,
        Stroke::new(1.0, theme::SEAM),
        egui::StrokeKind::Inside,
    );
    r.shrink(2.0)
}

/// The colours of a list row: the picked one is a black bar with bone print.
fn row_look(g: &egui::Painter, r: Rect, selected: bool, hovered: bool) -> (Color32, Color32) {
    if selected {
        g.rect_filled(r.shrink2(vec2(2.0, 1.0)), 3.0, theme::CAP);
        (theme::ON_DARK, theme::ON_DARK_DIM)
    } else {
        if hovered {
            g.rect_filled(r.shrink2(vec2(2.0, 1.0)), 3.0, theme::BONE_HI);
        }
        (theme::INK, theme::PRINT)
    }
}

fn number(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What the window asks of the project, done after it is drawn.
#[derive(Default)]
struct Act {
    load: bool,
    assign: Option<(usize, PathBuf)>,
}

/// The rack's channels as the pads show them: (name, has a sample).
fn rack_pads(p: &Project) -> Vec<(String, bool)> {
    channel_rack::rack_track(p)
        .and_then(|i| p.tracks[i].au_instrument.as_ref())
        .map(|slot| {
            channel_rack::decode(&slot.state)
                .into_iter()
                .map(|c| (c.name, !c.path.is_empty()))
                .collect()
        })
        .unwrap_or_default()
}

/// The Kit Browser's window.
pub fn show(ctx: &egui::Context, p: &mut Project, s: &mut crate::UiState) {
    let v = &mut s.kits;
    if !v.open {
        if v.audition_key().is_some() {
            v.stop_audition();
        }
        return;
    }
    v.start();
    v.poll();
    let name = v.selected_name();
    let title = tr("Kits").to_uppercase();
    let plate = gear::Plate {
        model: &title,
        name: &name,
        strip: None,
        reset: false,
    };
    let id = Id::unique("prowave_kit_browser");
    let pads = rack_pads(p);
    let mut act = Act::default();
    let (head, _) = gear::unit_window(ctx, id, WIDTH, &plate, None, |ui| {
        body(ui, id, v, &pads, &mut act);
    });
    if head.close {
        v.open = false;
        v.stop_audition();
    }
    if act.load {
        load_selected(v, p);
    }
    if let Some((channel, path)) = act.assign {
        assign(p, channel, &path);
    }
    // The sample being dragged, by the pointer.
    if let (Some(drag), Some(at)) = (
        egui::DragAndDrop::payload::<SampleDrag>(ctx),
        ctx.pointer_interact_pos(),
    ) {
        let g = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, id.with("drag")));
        let label = drag
            .0
            .file_stem()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let r = Rect::from_min_size(at + vec2(12.0, 10.0), vec2(170.0, 22.0));
        hw::paint_slot(
            &g,
            r,
            hw::SlotKind::Filled,
            &label,
            Some(true),
            false,
            false,
        );
    }
    if v.busy() {
        ctx.request_repaint_after(Duration::from_millis(40));
    }
}

fn body(ui: &mut Ui, id: Id, v: &mut KitBrowser, pads: &[(String, bool)], act: &mut Act) {
    let pads_h = 20.0 + 2.0 * 46.0;
    let total = 12.0 + 30.0 + 10.0 + (16.0 + LIST_H) + 14.0 + pads_h + 14.0 + 30.0;
    let (rect, _) = ui.allocate_exact_size(vec2(WIDTH, total), Sense::hover());
    let g = ui.painter().clone();
    let left = rect.left() + PAD;
    let right = rect.right() - PAD;
    let mut y = rect.top() + 12.0;

    // ---- the search field, the filters, the count
    let count_w = 112.0;
    let keys: [(&str, f32); 3] = [
        (tr("All"), 62.0),
        (tr("Best"), 68.0),
        (tr("Favorites"), 112.0),
    ];
    let keys_w: f32 = keys.iter().map(|k| k.1 + 6.0).sum();
    let label = tr("Search");
    let lw = hw::silk_width(&g, label, 10.5);
    hw::silk(
        &g,
        pos2(left + 2.0, y + 15.0),
        Align2::LEFT_CENTER,
        label,
        10.5,
        theme::INK2,
    );
    let field = Rect::from_min_max(
        pos2(left + lw + 12.0, y + 2.0),
        pos2(right - count_w - 10.0 - keys_w - 4.0, y + 28.0),
    );
    hw::paint_field(&g, field, "", false, false);
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field.shrink2(vec2(6.0, 4.0))));
    let edit = crate::hw_sampler::fields(&mut child, |ui| {
        ui.add(
            egui::TextEdit::singleline(&mut v.filter.query)
                .id(id.with("search"))
                .frame(egui::Frame::NONE)
                .font(theme::font(Face::Bold, 12.0))
                .text_color(theme::ON_DARK)
                .desired_width(field.width() - 12.0),
        )
    });
    let mut x = field.right() + 10.0;
    let lit = [
        !v.filter.best && !v.filter.favorites,
        v.filter.best,
        v.filter.favorites,
    ];
    for (k, (text, w)) in keys.iter().enumerate() {
        let r = Rect::from_min_size(pos2(x, y + 1.0), vec2(*w, 28.0));
        if gear::key(
            ui,
            r,
            id.with(("filter", k)),
            text,
            Cap::Bone,
            LedAt::Left,
            lit[k],
            lit[k],
        )
        .clicked()
        {
            v.filter.best = k == 1;
            v.filter.favorites = k == 2;
        }
        x += w + 6.0;
    }
    let total_kits = v.results().len();
    gear::lcd_chip(
        &g,
        Rect::from_min_max(pos2(right - count_w, y + 1.0), pos2(right, y + 29.0)),
        tr("Kits"),
        &total_kits.to_string(),
    );
    y += 30.0 + 10.0;

    // ---- the three lists, or the folders
    let lists = Rect::from_min_max(pos2(left, y), pos2(right, y + 16.0 + LIST_H));
    if v.show_folders {
        folders(ui, id, v, lists);
    } else {
        let sources = Rect::from_min_size(lists.min, vec2(SOURCES_W, lists.height()));
        let kits = Rect::from_min_size(
            pos2(sources.right() + GAP, lists.top()),
            vec2(KITS_W, lists.height()),
        );
        let kit = Rect::from_min_max(pos2(kits.right() + GAP, lists.top()), lists.max);
        source_list(ui, id, v, sources);
        kit_list(ui, id, v, kits, edit.has_focus(), act);
        kit_pane(ui, id, v, kit, act);
    }
    y = lists.bottom() + 14.0;

    // ---- the rack's channels as pads
    hw::bracket(&g, left, right, y + 8.0, tr("Channel Rack"));
    y += 20.0;
    let cols = KIT_PADS / 2;
    let pw = (right - left - (cols as f32 - 1.0) * 6.0) / cols as f32;
    for k in 0..KIT_PADS {
        let r = Rect::from_min_size(
            pos2(
                left + (k % cols) as f32 * (pw + 6.0),
                y + (k / cols) as f32 * 46.0,
            ),
            vec2(pw, 40.0),
        );
        let resp = ui.interact(r, id.with(("pad", k)), Sense::click());
        let selected = v.pad == k;
        let (name, filled) = pads
            .get(k)
            .map(|(n, f)| (n.as_str(), *f))
            .unwrap_or(("", false));
        let cap = if selected { Cap::Orange } else { Cap::Bone };
        let face = hw::paint_key(&g, r, cap, resp.is_pointer_button_down_on(), resp.hovered());
        hw::silk(
            &g,
            pos2(face.left() + 7.0, face.top() + 9.0),
            Align2::LEFT_CENTER,
            &format!("{:02}", k + 1),
            10.0,
            cap.ink(),
        );
        hw::led_sized(&g, pos2(face.right() - 9.0, face.top() + 9.0), 2.5, filled);
        let shown = hw::fit(&g, name, Face::Bold, 10.5, face.width() - 12.0);
        hw::text(
            &g,
            pos2(face.left() + 7.0, face.bottom() - 11.0),
            Align2::LEFT_CENTER,
            &shown.to_uppercase(),
            Face::Bold,
            10.5,
            cap.ink(),
        );
        if resp.dnd_hover_payload::<SampleDrag>().is_some() {
            g.rect_stroke(
                r,
                4.0,
                Stroke::new(2.0, theme::ORANGE),
                egui::StrokeKind::Inside,
            );
        }
        if let Some(drop) = resp.dnd_release_payload::<SampleDrag>() {
            act.assign = Some((k, drop.0.clone()));
            v.pad = k;
        }
        if resp.clicked() {
            v.pad = k;
        }
    }
    y += 2.0 * 46.0 + 14.0 - 6.0;

    // ---- the foot: Load Kit, Audition, Folders
    let can_load = v.kit.as_ref().is_some_and(|k| !k.pads.is_empty());
    let r = Rect::from_min_size(pos2(left, y), vec2(170.0, 30.0));
    if can_load {
        if gear::key(
            ui,
            r,
            id.with("load"),
            tr("Load Kit"),
            Cap::Orange,
            LedAt::None,
            false,
            false,
        )
        .clicked()
        {
            act.load = true;
        }
    } else {
        hu::key(
            ui,
            r,
            id.with("load_off"),
            Cap::Bone,
            false,
            None,
            &tr("Load Kit").to_uppercase(),
            false,
        );
    }
    let r = Rect::from_min_size(pos2(r.right() + 8.0, y), vec2(140.0, 30.0));
    let kit_key = v.selected.clone().map(|s| format!("kit:{s}"));
    let kit_playing = kit_key.is_some() && v.audition_key() == kit_key.as_deref();
    if can_load {
        if gear::key(
            ui,
            r,
            id.with("audition"),
            tr("Audition"),
            Cap::Bone,
            LedAt::Left,
            kit_playing,
            kit_playing,
        )
        .clicked()
        {
            if let (Some(kit), Some(key)) = (v.kit.clone(), kit_key) {
                let paths: Vec<PathBuf> = pad_paths(&kit).into_iter().flatten().collect();
                v.audition(key, paths, true);
            }
        }
    } else {
        hu::key(
            ui,
            r,
            id.with("audition_off"),
            Cap::Bone,
            false,
            Some(false),
            &tr("Audition").to_uppercase(),
            false,
        );
    }
    let r = Rect::from_min_max(pos2(right - 130.0, y), pos2(right, y + 30.0));
    if gear::key(
        ui,
        r,
        id.with("folders"),
        tr("Folders"),
        Cap::Bone,
        LedAt::Left,
        v.show_folders,
        v.show_folders,
    )
    .clicked()
    {
        v.show_folders = !v.show_folders;
    }
    // The lamp beside Folders is lit while the folders are being read.
    if v.scanning {
        hw::led_sized(&g, pos2(r.left() - 14.0, r.center().y), 3.0, true);
    }
}

fn source_list(ui: &mut Ui, id: Id, v: &mut KitBrowser, rect: Rect) {
    let g = ui.painter().clone();
    let inner = floor(&g, rect, tr("Source"));
    let all: usize = v.index.sources.iter().map(|s| s.1).sum();
    let n = v.index.sources.len() + 1;
    let mut pick: Option<Option<String>> = None;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    egui::ScrollArea::vertical()
        .id_salt(id.with("sources"))
        .auto_shrink([false, false])
        .show_rows(&mut child, ROW, n, |ui, range| {
            for i in range {
                let (r, resp) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
                let g = ui.painter();
                let (name, count, selected) = if i == 0 {
                    (tr("All").to_uppercase(), all, v.filter.source.is_none())
                } else {
                    let (s, c) = &v.index.sources[i - 1];
                    (s.clone(), *c, v.filter.source.as_ref() == Some(s))
                };
                let (ink, dim) = row_look(g, r, selected, resp.hovered());
                let num = number(count);
                let nr = hw::text(
                    g,
                    pos2(r.right() - 10.0, r.center().y),
                    Align2::RIGHT_CENTER,
                    &num,
                    Face::Bold,
                    10.0,
                    dim,
                );
                let shown = hw::fit(g, &name, Face::Bold, 11.0, nr.left() - r.left() - 20.0);
                hw::text(
                    g,
                    pos2(r.left() + 10.0, r.center().y),
                    Align2::LEFT_CENTER,
                    &shown,
                    Face::Bold,
                    11.0,
                    ink,
                );
                if resp.clicked() {
                    pick = Some(if i == 0 {
                        None
                    } else {
                        Some(v.index.sources[i - 1].0.clone())
                    });
                }
            }
        });
    if let Some(source) = pick {
        v.filter.source = source;
    }
}

fn kit_list(ui: &mut Ui, id: Id, v: &mut KitBrowser, rect: Rect, keys: bool, act: &mut Act) {
    let g = ui.painter().clone();
    let inner = floor(&g, rect, tr("Kit"));
    let results: Vec<u32> = v.results().to_vec();
    let at = v.selected.as_ref().and_then(|sel| {
        results
            .iter()
            .position(|i| &v.index.kits[*i as usize].path == sel)
    });
    // Arrows and Return while the search field has the keys.
    let mut pick: Option<String> = None;
    if keys && !results.is_empty() {
        let (down, up, enter) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowDown),
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::Enter),
            )
        });
        let to = match (down, up, at) {
            (true, _, Some(a)) => Some((a + 1).min(results.len() - 1)),
            (true, _, None) => Some(0),
            (_, true, Some(a)) => Some(a.saturating_sub(1)),
            _ => None,
        };
        if let Some(to) = to {
            pick = Some(v.index.kits[results[to] as usize].path.clone());
            v.scroll_to = Some(to);
        }
        if enter {
            act.load = true;
        }
    }
    let mut fav: Option<(String, bool)> = None;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    let mut area = egui::ScrollArea::vertical()
        .id_salt(id.with("kits"))
        .auto_shrink([false, false]);
    if let Some(row) = v.scroll_to.take() {
        let want = (row as f32 * ROW - inner.height() / 2.0 + ROW / 2.0).max(0.0);
        area = area.vertical_scroll_offset(want);
    }
    area.show_rows(&mut child, ROW, results.len(), |ui, range| {
        for row in range {
            let k = &v.index.kits[results[row] as usize];
            let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
            let star_r = Rect::from_min_size(r.min, vec2(26.0, ROW));
            let star_resp = ui.interact(star_r, id.with(("fav", row)), Sense::click());
            let g = ui.painter();
            let selected = v.selected.as_deref() == Some(k.path.as_str());
            let (ink, dim) = row_look(g, r, selected, resp.hovered() || star_resp.hovered());
            let is_fav = v.settings.favorites.contains(&k.path);
            star(
                g,
                star_r.center(),
                6.0,
                is_fav,
                if is_fav { theme::ORANGE } else { dim },
            );
            let mut text_right = r.right() - 10.0;
            if let Some(n) = k.files {
                let nr = hw::text(
                    g,
                    pos2(text_right, r.center().y),
                    Align2::RIGHT_CENTER,
                    &number(n as usize),
                    Face::Bold,
                    10.0,
                    dim,
                );
                text_right = nr.left() - 8.0;
            }
            let shown = hw::fit(
                g,
                &k.name,
                Face::Bold,
                11.0,
                text_right - star_r.right() - 2.0,
            );
            hw::text(
                g,
                pos2(star_r.right() + 2.0, r.center().y),
                Align2::LEFT_CENTER,
                &shown,
                Face::Bold,
                11.0,
                ink,
            );
            if star_resp.clicked() {
                fav = Some((k.path.clone(), !is_fav));
            } else if resp.double_clicked() {
                pick = Some(k.path.clone());
                act.load = true;
            } else if resp.clicked() {
                pick = Some(k.path.clone());
            }
        }
    });
    if let Some((path, on)) = fav {
        v.set_favorite(&path, on);
    }
    if let Some(path) = pick {
        v.select(&path);
    }
}

/// The kit picked: its name, source and license on an LCD, the classes it holds as keys, and
/// its samples.
fn kit_pane(ui: &mut Ui, id: Id, v: &mut KitBrowser, rect: Rect, act: &mut Act) {
    let g = ui.painter().clone();
    let entry = v
        .selected
        .as_ref()
        .and_then(|sel| v.index.kits.iter().find(|k| &k.path == sel))
        .cloned();
    let glass = Rect::from_min_max(
        pos2(rect.left(), rect.top() + 2.0),
        pos2(rect.right(), rect.top() + 56.0),
    );
    gear::glass(&g, glass);
    let kit = v.kit.clone();
    if let Some(e) = &entry {
        let count = kit
            .as_ref()
            .map(|k| k.samples.len())
            .or(e.files.map(|n| n as usize));
        let mut name_right = glass.right() - 8.0;
        if let Some(n) = count {
            let text = n.to_string();
            let h = 16.0;
            let w = gear::seg7_width(&text, h);
            gear::seg7(
                &g,
                pos2(glass.right() - 8.0 - w, glass.top() + 8.0),
                &text,
                h,
                theme::LCD_INK,
                false,
            );
            name_right -= w + 10.0;
        }
        let name = gear::fit(&g, &e.name, 13.0, name_right - glass.left() - 8.0);
        gear::lcd_text(
            &g,
            pos2(glass.left() + 8.0, glass.top() + 16.0),
            Align2::LEFT_CENTER,
            &name,
            13.0,
        );
        let line = if e.license.is_empty() {
            e.source.clone()
        } else {
            format!("{}  ·  {}", e.source, e.license)
        };
        let line = gear::fit(&g, &line, 9.0, glass.width() - 16.0);
        gear::lcd_label(
            &g,
            pos2(glass.left() + 8.0, glass.bottom() - 13.0),
            Align2::LEFT_CENTER,
            &line,
        );
    }
    // The classes the kit holds: a key each, lit when the list shows that class alone.
    let mut y = glass.bottom() + 8.0;
    let mut classes: Vec<(DrumClass, usize)> = Vec::new();
    if let Some(k) = &kit {
        for c in DrumClass::ALL {
            let n = k.samples.iter().filter(|s| s.class == c).count();
            if n > 0 {
                classes.push((c, n));
            }
        }
    }
    if classes.len() > 1 {
        let mut x = rect.left();
        for (c, _) in &classes {
            let w = hw::silk_width(&g, c.tag(), 9.5) + 18.0;
            if x + w > rect.right() {
                x = rect.left();
                y += 24.0;
            }
            let r = Rect::from_min_size(pos2(x, y), vec2(w, 20.0));
            let on = v.class == Some(*c);
            if gear::key(
                ui,
                r,
                id.with(("class", c.id())),
                c.tag(),
                if on { Cap::Dark } else { Cap::Bone },
                LedAt::None,
                false,
                on,
            )
            .clicked()
            {
                v.class = if on { None } else { Some(*c) };
            }
            x += w + 4.0;
        }
        y += 24.0;
    }
    let list = Rect::from_min_max(pos2(rect.left(), y + 2.0), rect.max);
    let inner = floor(&g, list, tr("Samples"));
    let Some(kit) = kit else {
        return;
    };
    let shown: Vec<usize> = (0..kit.samples.len())
        .filter(|i| v.class.is_none_or(|c| kit.samples[*i].class == c))
        .collect();
    let playing = v.audition_key().map(str::to_string);
    let mut hear: Option<usize> = None;
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    child.set_clip_rect(inner.intersect(ui.clip_rect()));
    child.spacing_mut().item_spacing = vec2(0.0, 0.0);
    egui::ScrollArea::vertical()
        .id_salt(id.with(("samples", v.selected.as_deref().unwrap_or(""))))
        .auto_shrink([false, false])
        .show_rows(&mut child, ROW, shown.len(), |ui, range| {
            for row in range {
                let i = shown[row];
                let sample = &kit.samples[i];
                let (r, resp) = ui
                    .allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click_and_drag());
                let well =
                    Rect::from_center_size(pos2(r.left() + 17.0, r.center().y), vec2(20.0, 20.0));
                let hear_resp = ui.interact(well, id.with(("hear", i)), Sense::click());
                let g = ui.painter();
                let on_pad = kit.pads.contains(&Some(i));
                let (ink, _) = row_look(g, r, false, resp.hovered() || hear_resp.hovered());
                let path = kit.path_of(i);
                let key = format!("sample:{}", path.display());
                hu::well(g, well);
                play_glyph(
                    g,
                    well.center(),
                    playing.as_deref() == Some(key.as_str()),
                    theme::INK,
                );
                let tag = hu::tag(
                    g,
                    pos2(
                        r.right() - 8.0 - hw::silk_width(g, sample.class.tag(), 8.5) - 10.0,
                        r.center().y,
                    ),
                    sample.class.tag(),
                    8.5,
                );
                // A lamp on the samples a kit load puts on the channels.
                if on_pad {
                    hw::led_sized(g, pos2(tag.left() - 9.0, r.center().y), 2.5, true);
                }
                let name_x = well.right() + 8.0;
                let name = hw::fit(
                    g,
                    &sample.name,
                    Face::Bold,
                    11.0,
                    tag.left() - 18.0 - name_x,
                );
                hw::text(
                    g,
                    pos2(name_x, r.center().y),
                    Align2::LEFT_CENTER,
                    &name,
                    Face::Bold,
                    11.0,
                    ink,
                );
                if hear_resp.clicked() {
                    hear = Some(i);
                } else if resp.double_clicked() {
                    act.assign = Some((v.pad, path.clone()));
                }
                if resp.drag_started() {
                    resp.dnd_set_drag_payload(SampleDrag(path));
                }
            }
        });
    if let Some(i) = hear {
        let path = kit.path_of(i);
        v.audition(format!("sample:{}", path.display()), vec![path], false);
    }
}

/// The library folders: a lamp (lit when the folder is there), the folder, its kits, Remove;
/// then Add Folder and Rescan, and what the last scan found.
fn folders(ui: &mut Ui, id: Id, v: &mut KitBrowser, rect: Rect) {
    let g = ui.painter().clone();
    let inner = floor(&g, rect, tr("Folders"));
    let mut remove: Option<usize> = None;
    let mut y = inner.top() + 6.0;
    let folders = v.settings.folders.clone();
    for (i, f) in folders.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(inner.left() + 6.0, y),
            vec2(inner.width() - 12.0, 30.0),
        );
        let info = v.index.roots.iter().find(|r| &r.path == f);
        let present = info.map_or_else(|| Path::new(f).is_dir(), |r| r.present);
        hw::led_sized(&g, pos2(r.left() + 10.0, r.center().y), 3.5, present);
        let key = Rect::from_min_max(
            pos2(r.right() - 96.0, r.top() + 2.0),
            pos2(r.right(), r.bottom() - 2.0),
        );
        let count = Rect::from_min_max(
            pos2(key.left() - 104.0, r.top() + 2.0),
            pos2(key.left() - 8.0, r.bottom() - 2.0),
        );
        if present {
            gear::lcd_chip(&g, count, "", &info.map_or(0, |r| r.kits).to_string());
        }
        let shown = hw::fit(&g, f, Face::Bold, 11.5, count.left() - r.left() - 34.0);
        hw::text(
            &g,
            pos2(r.left() + 24.0, r.center().y),
            Align2::LEFT_CENTER,
            &shown,
            Face::Bold,
            11.5,
            if present { theme::INK } else { theme::PRINT },
        );
        if gear::key(
            ui,
            key,
            id.with(("remove", i)),
            tr("Remove"),
            Cap::Bone,
            LedAt::None,
            false,
            false,
        )
        .clicked()
        {
            remove = Some(i);
        }
        g.hline(
            r.left()..=r.right(),
            r.bottom() + 2.5,
            Stroke::new(1.0, Color32::from_black_alpha(22)),
        );
        y += 36.0;
    }
    y += 6.0;
    let add = Rect::from_min_size(pos2(inner.left() + 6.0, y), vec2(150.0, 28.0));
    if gear::key(
        ui,
        add,
        id.with("add_folder"),
        tr("Add Folder…"),
        Cap::Bone,
        LedAt::None,
        false,
        false,
    )
    .clicked()
    {
        if let Some(dir) = rfd::FileDialog::new().pick_folder() {
            let mut list = v.settings.folders.clone();
            list.push(dir.to_string_lossy().into_owned());
            v.set_folders(list);
        }
    }
    let rescan = Rect::from_min_size(pos2(add.right() + 8.0, y), vec2(120.0, 28.0));
    if gear::key(
        ui,
        rescan,
        id.with("rescan"),
        tr("Rescan"),
        Cap::Bone,
        LedAt::Left,
        v.scanning,
        false,
    )
    .clicked()
    {
        v.rescan();
    }
    if let Some(took) = v.last_scan {
        let ms = took.as_secs_f64() * 1000.0;
        let text = if ms < 10.0 {
            format!("{ms:.1} ms")
        } else {
            format!("{ms:.0} ms")
        };
        gear::lcd_chip(
            &g,
            Rect::from_min_size(pos2(inner.right() - 6.0 - 120.0, y), vec2(120.0, 28.0)),
            "",
            &text,
        );
    }
    if let Some(i) = remove {
        let mut list = v.settings.folders.clone();
        list.remove(i);
        v.set_folders(list);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("prowave-kitui-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_wav(path: &Path, seconds: f32, hz: f32) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let n = (seconds * 44_100.0) as usize;
        let stereo: Vec<f32> = (0..n)
            .flat_map(|i| {
                let env = 1.0 - i as f32 / n as f32;
                let y = (i as f32 * hz * std::f32::consts::TAU / 44_100.0).sin() * env * 0.6;
                [y, y]
            })
            .collect();
        prowave_engine::write_wav(path, &stereo, 44_100).unwrap();
    }

    /// A small library of real audio files: two sources, three kits.
    fn library(name: &str) -> PathBuf {
        let dir = temp(name);
        for (kit, files) in [
            (
                "Machines/Roland TR-808",
                vec![
                    "BD/BD0000.wav",
                    "SD/SD0010.wav",
                    "CP/CP.wav",
                    "CH/CH.wav",
                    "OH/OH00.wav",
                    "RS/RS.wav",
                    "LT/LT00.wav",
                    "MT/MT00.wav",
                    "HT/HT00.wav",
                    "CB/CB.wav",
                    "MA/MA.wav",
                    "CY/CY0000.wav",
                    "BD/BD0025.wav",
                    "SD/SD0025.wav",
                    "CL/CL.wav",
                    "HC/HC00.wav",
                    "LC/LC00.wav",
                    "Loops/120bpm/Beat 1.wav",
                ],
            ),
            ("Machines/LinnDrum", vec!["kick.wav", "sd.wav", "chh.wav"]),
            (
                "Packs/Dusty Breaks Vol 2",
                vec!["Kicks/Kick 1.wav", "Snares/Snare 1.wav"],
            ),
        ] {
            for (i, f) in files.iter().enumerate() {
                write_wav(&dir.join(kit).join(f), 0.12, 80.0 + 60.0 * i as f32);
            }
        }
        dir
    }

    fn wait(v: &mut KitBrowser) {
        for _ in 0..2000 {
            v.poll();
            if !v.scanning && v.kit_rx.is_none() {
                return;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("the background work did not finish");
    }

    #[test]
    fn a_kit_is_found_read_and_loaded_into_a_new_rack() {
        let lib = library("load");
        let data = temp("load-data");
        let mut v = KitBrowser::with(Some(data.clone()), vec![lib.to_string_lossy().into_owned()]);
        v.rescan();
        wait(&mut v);
        assert_eq!(v.index.kits.len(), 3);
        assert!(data.join("index.json").is_file(), "the index is cached");
        v.filter.query = "808".into();
        let hits = v.results().to_vec();
        assert_eq!(hits.len(), 1);
        let path = v.index.kits[hits[0] as usize].path.clone();
        v.select(&path);
        wait(&mut v);
        let kit = v.kit().unwrap().clone();
        assert_eq!(kit.samples.len(), 18);
        assert_eq!(kit.pads.len(), KIT_PADS);
        let mut p = Project::empty();
        let ti = load_selected(&mut v, &mut p).unwrap();
        let slot = p.tracks[ti].au_instrument.clone().unwrap();
        assert_eq!(channel_rack::kit_of(&slot.state), "Roland TR-808");
        let names: Vec<String> = channel_rack::decode(&slot.state)
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(
            names,
            [
                "BD0000", "SD0010", "CP", "CH", "OH00", "RS", "LT00", "MT00", "HT00", "CB", "MA",
                "CY0000", "BD0025", "SD0025", "CL", "HC00"
            ]
        );
        // The engine's rack reads every one of them.
        let specs = channel_rack::specs(&slot);
        let (_, errors) = prowave_engine::SampleRack::new(&specs, 48_000);
        assert!(errors.is_empty(), "{errors:?}");
        // The files are named where they are: nothing is copied.
        assert!(specs[0].path.starts_with(&*lib.to_string_lossy()));
        // The count learnt by opening the kit is in the index.
        assert_eq!(v.index.kits[hits[0] as usize].files, Some(18));
        // One sample onto a channel past the kit's.
        assert!(assign(&mut p, 16, &kit.path_of(0)));
        assert_eq!(
            channel_rack::decode(&p.tracks[ti].au_instrument.as_ref().unwrap().state).len(),
            17
        );
        let _ = std::fs::remove_dir_all(lib);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn favourites_and_folders_are_kept() {
        let lib = library("fav");
        let data = temp("fav-data");
        let root = lib.to_string_lossy().into_owned();
        let mut v = KitBrowser::with(Some(data.clone()), vec![root.clone()]);
        v.rescan();
        wait(&mut v);
        let path = format!("{root}/Machines/LinnDrum");
        v.set_favorite(&path, true);
        v.filter.favorites = true;
        assert_eq!(v.results().len(), 1);
        v.set_favorite(&path, false);
        assert!(v.results().is_empty());
        v.set_favorite(&path, true);
        // A folder that is not there beside one that is: its kits are simply not listed.
        v.set_folders(vec!["/Volumes/NOT-MOUNTED-HERE/KITS".into(), root.clone()]);
        wait(&mut v);
        assert_eq!(v.index.kits.len(), 3);
        assert!(!v.index.roots[0].present && v.index.roots[1].present);
        assert!(!v.show_folders);
        let saved = Settings::load(&data);
        assert_eq!(saved.folders.len(), 2);
        assert!(saved.favorites.contains(&path));
        // Only the missing folder: no error, an empty list, and Folders shown.
        v.set_folders(vec!["/Volumes/NOT-MOUNTED-HERE/KITS".into()]);
        wait(&mut v);
        assert!(v.index.kits.is_empty() && v.show_folders && v.error.is_none());
        let _ = std::fs::remove_dir_all(lib);
        let _ = std::fs::remove_dir_all(data);
    }

    #[test]
    fn an_audition_is_the_sound_and_then_silence_to_stop_in() {
        let lib = library("hear");
        let kick = lib.join("Machines/LinnDrum/kick.wav");
        let (buf, end) = sample_preview(&kick).unwrap();
        let sound = buf.frames() - (TAIL_SECONDS * 44_100.0) as usize;
        assert!((5290..=5294).contains(&sound), "{sound}");
        assert!((end - sound as f32 / buf.frames() as f32).abs() < 1e-6);
        assert!(buf.data[sound * 2..].iter().all(|s| *s == 0.0));
        assert!(buf.data[..sound * 2].iter().any(|s| s.abs() > 0.1));
        // A kit: its hits one after the other.
        let paths = vec![
            kick.clone(),
            lib.join("nothing.wav"),
            lib.join("Machines/LinnDrum/sd.wav"),
        ];
        let (mix, end) = kit_preview(&paths).unwrap();
        let second = (KIT_STEP_SECONDS * 44_100.0) as usize;
        assert!(mix.data[second * 2 + 200..second * 2 + 2000]
            .iter()
            .any(|s| s.abs() > 0.1));
        assert!(end > 0.3 && end < 1.0);
        assert!(kit_preview(&[]).is_err());
        // Without an engine nothing is left waiting.
        let mut v = KitBrowser::with(None, Vec::new());
        v.audition("sample:x".into(), vec![kick], false);
        for _ in 0..500 {
            pump(&mut v, None);
            if v.want.is_none() {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(v.audition_key().is_none() && !v.busy());
        let _ = std::fs::remove_dir_all(lib);
    }

    /// The window with a kit picked, the folders, and the rack with a kit loaded.
    #[test]
    #[ignore = "pictures"]
    fn kit_browser_pictures() {
        let lib = library("pictures");
        let root = lib.to_string_lossy().into_owned();
        std::fs::write(
            lib.join("MANIFEST.tsv"),
            "source\tkit\tlicense\taudio_or_instrument_files\n\
             Machines\tRoland TR-808\tCC BY-NC-ND 4.0\t18\n\
             Machines\tLinnDrum\tCC BY-NC-ND 4.0\t3\n\
             Packs\tDusty Breaks Vol 2\tRoyalty free\t2\n",
        )
        .unwrap();
        std::fs::write(lib.join("BEST.md"), "1. Machines / Roland TR-808\n").unwrap();
        let mut p = Project::empty();
        let mut s = crate::UiState::new();
        s.kits = KitBrowser::with(
            None,
            vec![root.clone(), "/Volumes/NOT-MOUNTED-HERE/DRUM-KITS".into()],
        );
        s.kits.open = true;
        s.kits.rescan();
        wait(&mut s.kits);
        let path = format!("{root}/Machines/Roland TR-808");
        s.kits.select(&path);
        wait(&mut s.kits);
        s.kits.set_favorite(&path, true);
        load_selected(&mut s.kits, &mut p);
        s.kits.pad = 2;
        let dir = std::env::var_os("PROWAVE_UI_PNG_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        std::fs::create_dir_all(&dir).unwrap();
        let shot = |name: &str, p: &mut Project, s: &mut crate::UiState| {
            crate::ui_test::render_png(
                980.0,
                760.0,
                2.0,
                crate::theme::BONE3,
                |ui| {
                    crate::theme::apply(ui.ctx());
                    show(ui.ctx(), p, s)
                },
                &dir.join(name),
            )
            .unwrap();
        };
        shot("kit-browser.png", &mut p, &mut s);
        s.kits.show_folders = true;
        shot("kit-browser-folders.png", &mut p, &mut s);
        s.kits.show_folders = false;
        s.kits.filter.query = "linn".into();
        shot("kit-browser-search.png", &mut p, &mut s);
        crate::ui_test::render_png(
            1200.0,
            640.0,
            2.0,
            crate::theme::BONE,
            |ui| {
                crate::theme::apply(ui.ctx());
                channel_rack::show(ui, &mut p, &mut s)
            },
            &dir.join("kit-browser-rack.png"),
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(lib);
    }
}
