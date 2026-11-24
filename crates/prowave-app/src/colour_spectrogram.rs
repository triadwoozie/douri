//! View > Colour Spectrogram: audio regions drawn as dadaspec's colour spectrogram instead of
//! a waveform, so a song's sections show as changes of colour (bass red, mid green, high blue;
//! the left channel in the top half, the right in the bottom half, bass meeting in the middle).
//! The analysis is `prowave_engine::colour_spectrum` (a port of dadaspec, MIT; see there).
//!
//! The picture of a file is made once, off the interface thread: up to [`WORKERS`] threads
//! take the files the drawing asks for, in the order it asks. A finished picture is kept in
//! memory and on disk under the hash of the file's bytes (`<cache>/colour-spectrogram/`), so
//! the same recording under another name, or the next time the project is opened, is not
//! measured again. Until a file's picture is there its region shows the waveform: the drawing
//! never waits.
//!
//! On screen a picture is a set of textures, one or more per detail level: level 0 has one
//! column per 64 ms frame, each next level averages pairs of the one before, and a region is
//! drawn from the level with one to two columns per pixel, so a whole song zoomed out does
//! not shimmer. A picture wider than [`TILE`] columns is cut into textures that overlap by one
//! column, so the smoothing across a seam is the same as anywhere else.
use egui::{pos2, Color32, Mesh, Painter, Rect, TextureHandle, TextureOptions};
use prowave_engine::{AudioFile, ColourAnalyser, ColourSpectrum};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, Weak};

/// Files measured at once.
const WORKERS: usize = 2;
/// Pictures kept in memory (about 0.7 MB for four minutes of sound).
const KEEP: usize = 64;
/// Pictures kept as textures.
const KEEP_TEXTURES: usize = 48;
/// Columns a texture holds besides its two overlap columns.
const TILE: usize = 2046;
/// The disk cache file's first bytes, and the version of the analysis: a change of the method
/// changes the version, and old files are not read.
const MAGIC: &[u8; 4] = b"PWCS";
const VERSION: u32 = 1;
/// Cached pictures not used for this long are deleted, in days.
const CACHE_DAYS: u64 = 30;
/// The width of the strips a region is drawn in, in points: each strip maps its stretch of
/// screen to its stretch of the file in a straight line, so a tempo change bends the picture
/// as it bends the waveform.
const STRIP: f32 = 4.0;

/// Which file a picture is asked for: its path, length and rate (a recording that grows asks
/// again).
type Key = (PathBuf, u64, u32);

enum Slot {
    Waiting,
    Ready {
        hash: u64,
        picture: Arc<ColourSpectrum>,
        used: u64,
    },
    Failed,
}

#[derive(Default)]
struct Inner {
    slots: HashMap<Key, Slot>,
    queue: VecDeque<(Key, AudioFile)>,
    workers: usize,
    /// Counts the asks, for keeping the most recently used pictures.
    tick: u64,
    /// Pictures by the hash of their file's bytes, while any slot holds them.
    by_hash: HashMap<u64, Weak<ColourSpectrum>>,
    /// Told to draw again when a picture is ready.
    ctx: Option<egui::Context>,
}

/// The pictures made and being made.
pub(crate) struct Cache {
    inner: Mutex<Inner>,
    /// The disk cache folder.
    dir: PathBuf,
    /// Files measured (not found in memory or on disk).
    pub(crate) measured: AtomicUsize,
    /// Pictures read from the disk cache.
    pub(crate) from_disk: AtomicUsize,
}

impl Cache {
    pub(crate) fn new(dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner::default()),
            dir,
            measured: AtomicUsize::new(0),
            from_disk: AtomicUsize::new(0),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The picture of `file` and its content hash if it is made; otherwise `None`, and the
    /// file is queued to be measured (once). `ctx` is asked to draw again when it is ready.
    pub(crate) fn get(
        self: &Arc<Self>,
        ctx: Option<&egui::Context>,
        file: &AudioFile,
    ) -> Option<(u64, Arc<ColourSpectrum>)> {
        let key: Key = (file.path.clone(), file.frames, file.sample_rate);
        let mut inner = self.lock();
        inner.tick += 1;
        let tick = inner.tick;
        if let Some(c) = ctx {
            if inner.ctx.is_none() {
                inner.ctx = Some(c.clone());
            }
        }
        match inner.slots.get_mut(&key) {
            Some(Slot::Ready {
                hash,
                picture,
                used,
            }) => {
                *used = tick;
                return Some((*hash, picture.clone()));
            }
            Some(Slot::Waiting | Slot::Failed) => return None,
            None => {}
        }
        inner.slots.insert(key.clone(), Slot::Waiting);
        inner.queue.push_back((key, file.clone()));
        if inner.workers < WORKERS {
            inner.workers += 1;
            let me = self.clone();
            let spawned = std::thread::Builder::new()
                .name("prowave-colour-spectrogram".into())
                .spawn(move || me.work());
            if spawned.is_err() {
                inner.workers -= 1;
            }
        }
        None
    }

    /// A worker: measures queued files until there are none.
    fn work(self: Arc<Self>) {
        loop {
            let job = {
                let mut inner = self.lock();
                let job = inner.queue.pop_front();
                if job.is_none() {
                    inner.workers -= 1;
                }
                job
            };
            let Some((key, file)) = job else {
                return;
            };
            let done = self.make(&file);
            let ctx = {
                let mut inner = self.lock();
                let slot = match done {
                    Ok((hash, picture)) => {
                        inner.by_hash.insert(hash, Arc::downgrade(&picture));
                        Slot::Ready {
                            hash,
                            picture,
                            used: inner.tick,
                        }
                    }
                    Err(e) => {
                        tracing::warn!(
                            "colour spectrogram failed for {}: {e}",
                            file.path.display()
                        );
                        Slot::Failed
                    }
                };
                inner.slots.insert(key, slot);
                forget_old(&mut inner);
                inner.ctx.clone()
            };
            if let Some(c) = ctx {
                c.request_repaint();
            }
        }
    }

    /// The picture of `file`: from memory under the same content, from the disk cache, or
    /// measured now (slow).
    fn make(&self, file: &AudioFile) -> Result<(u64, Arc<ColourSpectrum>), String> {
        let hash = crate::pitch_editor::content_hash(&file.path).map_err(|e| e.to_string())?;
        if let Some(p) = self.lock().by_hash.get(&hash).and_then(Weak::upgrade) {
            return Ok((hash, p));
        }
        let path = self.dir.join(format!("{hash:016x}.bin"));
        if let Some(p) = std::fs::read(&path).ok().and_then(|b| decode(&b)) {
            self.from_disk.fetch_add(1, Ordering::Relaxed);
            // Touched, so a picture in use is not cleaned away.
            let _ = std::fs::File::options()
                .append(true)
                .open(&path)
                .and_then(|f| f.set_modified(std::time::SystemTime::now()));
            return Ok((hash, Arc::new(p)));
        }
        clean(&self.dir);
        let picture = measure(file).map_err(|e| e.to_string())?;
        self.measured.fetch_add(1, Ordering::Relaxed);
        if std::fs::create_dir_all(&self.dir).is_ok() {
            let tmp = path.with_extension(format!("part{}", std::process::id()));
            if std::fs::write(&tmp, encode(&picture)).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
        Ok((hash, Arc::new(picture)))
    }
}

/// Lets go of the least recently used pictures beyond [`KEEP`].
fn forget_old(inner: &mut Inner) {
    let mut ready: Vec<(u64, Key)> = inner
        .slots
        .iter()
        .filter_map(|(k, s)| match s {
            Slot::Ready { used, .. } => Some((*used, k.clone())),
            _ => None,
        })
        .collect();
    if ready.len() <= KEEP {
        return;
    }
    ready.sort_by_key(|(used, _)| *used);
    for (_, k) in ready.iter().take(ready.len() - KEEP) {
        inner.slots.remove(k);
    }
    inner.by_hash.retain(|_, w| w.strong_count() > 0);
}

/// Measures `file` (decoding it in pieces: the sound is never all in memory).
pub(crate) fn measure(file: &AudioFile) -> Result<ColourSpectrum, prowave_engine::AudioError> {
    let mut a = ColourAnalyser::new(file.sample_rate, file.frames);
    file.for_each_stereo(|piece| {
        a.push(piece);
        true
    })?;
    Ok(a.finish())
}

fn encode(p: &ColourSpectrum) -> Vec<u8> {
    let mut b = Vec::with_capacity(16 + p.rgb.len());
    b.extend_from_slice(MAGIC);
    b.extend_from_slice(&VERSION.to_le_bytes());
    b.extend_from_slice(&(p.rows as u32).to_le_bytes());
    b.extend_from_slice(&(p.frames as u32).to_le_bytes());
    b.extend_from_slice(&p.rgb);
    b
}

fn decode(b: &[u8]) -> Option<ColourSpectrum> {
    let word = |i: usize| -> Option<usize> {
        Some(u32::from_le_bytes(b.get(i..i + 4)?.try_into().ok()?) as usize)
    };
    if b.get(0..4)? != MAGIC || word(4)? != VERSION as usize {
        return None;
    }
    let (rows, frames) = (word(8)?, word(12)?);
    let rgb = b.get(16..)?;
    (rows > 0 && rgb.len() == rows.checked_mul(frames)?.checked_mul(3)?).then(|| ColourSpectrum {
        frames,
        rows,
        rgb: rgb.to_vec(),
    })
}

/// Deletes cached pictures not used for [`CACHE_DAYS`], once a run.
fn clean(dir: &Path) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > CACHE_DAYS * 86_400);
            if old {
                let _ = std::fs::remove_file(e.path());
            }
        }
    });
}

/// The app's cache.
fn global() -> &'static Arc<Cache> {
    static C: OnceLock<Arc<Cache>> = OnceLock::new();
    C.get_or_init(|| Cache::new(crate::flex_edit::cache_dir().join("colour-spectrogram")))
}

// ------------------------------------------------------------------ the switch

fn on_id() -> egui::Id {
    egui::Id::unique("colour_spectrogram_on")
}

/// Publishes this frame's View > Colour Spectrogram for the region drawing.
pub(crate) fn publish(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(on_id(), on));
}

/// Whether audio regions are drawn as colour spectrograms this frame.
pub(crate) fn on(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(on_id())).unwrap_or(false)
}

fn setting_file() -> Option<PathBuf> {
    crate::recent_files::config_dir().map(|d| d.join("colour-spectrogram"))
}

/// View > Colour Spectrogram as it was left (off when it never was on).
pub(crate) fn load_on() -> bool {
    if cfg!(test) {
        return false;
    }
    setting_file()
        .and_then(|f| std::fs::read_to_string(f).ok())
        .is_some_and(|t| t.trim() == "on")
}

/// Remembers View > Colour Spectrogram for the next start.
pub(crate) fn save_on(on: bool) {
    // The tests of the menus must not change the settings of whoever runs them.
    if cfg!(test) {
        return;
    }
    if let Some(f) = setting_file() {
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir)
                .and_then(|_| std::fs::write(f, if on { "on" } else { "off" }));
        }
    }
}

// ------------------------------------------------------------------ textures

/// One texture of a level: columns `start..end` of the level, of which it is drawn for
/// `start + 1..end - 1` except at the picture's own ends (the others overlap its neighbours).
struct Tile {
    tex: TextureHandle,
    start: usize,
    end: usize,
    /// The level columns this texture is drawn for.
    from: usize,
    to: usize,
}

struct Levels {
    /// Level `l` has a column for every 2^`l` frames; made when first drawn.
    levels: Vec<Option<Vec<Tile>>>,
    used: u64,
}

#[derive(Default)]
struct Textures {
    by_hash: HashMap<u64, Levels>,
    tick: u64,
}

fn textures(ctx: &egui::Context) -> Arc<Mutex<Textures>> {
    ctx.data_mut(|d| {
        d.get_temp_mut_or_default::<Arc<Mutex<Textures>>>(egui::Id::unique(
            "colour_spectrogram_textures",
        ))
        .clone()
    })
}

/// Columns in level `l` of a picture `frames` wide.
fn level_width(frames: usize, l: usize) -> usize {
    frames.div_ceil(1 << l).max(1)
}

/// The colour of column `c` of level `l` (the average of its frames), row `row`.
fn level_pixel(p: &ColourSpectrum, l: usize, row: usize, c: usize) -> Color32 {
    let (a, b) = (c << l, ((c + 1) << l).min(p.frames));
    let mut sum = [0u32; 3];
    for f in a..b {
        let px = p.at(row, f);
        for k in 0..3 {
            sum[k] += u32::from(px[k]);
        }
    }
    let n = (b - a).max(1) as u32;
    Color32::from_rgb(
        ((sum[0] + n / 2) / n) as u8,
        ((sum[1] + n / 2) / n) as u8,
        ((sum[2] + n / 2) / n) as u8,
    )
}

fn make_level(ctx: &egui::Context, hash: u64, p: &ColourSpectrum, l: usize) -> Vec<Tile> {
    let width = level_width(p.frames, l);
    let mut tiles = Vec::new();
    let mut from = 0;
    while from < width {
        let to = (from + TILE).min(width);
        let (start, end) = (from.saturating_sub(1), (to + 1).min(width));
        let w = end - start;
        let mut pixels = Vec::with_capacity(w * p.rows);
        for row in 0..p.rows {
            for c in start..end {
                pixels.push(level_pixel(p, l, row, c));
            }
        }
        let img = egui::ColorImage::new([w, p.rows], pixels);
        let tex = ctx.load_texture(
            format!("colour-spectrogram-{hash:016x}-{l}-{from}"),
            img,
            TextureOptions::LINEAR,
        );
        tiles.push(Tile {
            tex,
            start,
            end,
            from,
            to,
        });
        from = to;
    }
    tiles
}

// ------------------------------------------------------------------ drawing

/// Draws the colour spectrogram of `file` over `area` and returns `true`, or returns `false`
/// when its picture is not made yet (it is then being made; draw the waveform meanwhile).
/// `file_secs(x)` is the second of the file heard at `x` on screen, `None` where nothing of it
/// plays. `tint` multiplies the picture (white: as it is).
pub(crate) fn paint(
    g: &Painter,
    area: Rect,
    file: &AudioFile,
    file_secs: impl Fn(f32) -> Option<f64>,
    tint: Color32,
) -> bool {
    paint_from(global(), g, area, file, file_secs, tint)
}

/// The app's picture of `file` if it is made (it is queued otherwise).
#[cfg(test)]
pub(crate) fn paint_ready(file: &AudioFile) -> Option<(u64, Arc<ColourSpectrum>)> {
    global().get(None, file)
}

pub(crate) fn paint_from(
    cache: &Arc<Cache>,
    g: &Painter,
    area: Rect,
    file: &AudioFile,
    file_secs: impl Fn(f32) -> Option<f64>,
    tint: Color32,
) -> bool {
    let ctx = g.ctx();
    let Some((hash, picture)) = cache.get(Some(ctx), file) else {
        return false;
    };
    let shown = area.intersect(g.clip_rect());
    if shown.width() <= 0.0 || shown.height() <= 0.0 {
        return true;
    }
    let strips = strips(shown.left(), shown.right(), &file_secs);
    if strips.is_empty() {
        return true;
    }
    // The level: one to two columns per pixel.
    let (secs, width) = strips.iter().fold((0.0, 0.0), |(s, w), st| {
        (s + (st.3 - st.2).abs(), w + (st.1 - st.0))
    });
    let px = f64::from(width * g.pixels_per_point()).max(1.0);
    let per_px = secs / ColourSpectrum::column_secs() / px;
    let max_level = usize::BITS as usize - 1 - picture.frames.max(1).leading_zeros() as usize;
    let level = if per_px >= 2.0 {
        (per_px.log2().floor() as usize).min(max_level)
    } else {
        0
    };
    let store = textures(ctx);
    let mut t = store.lock().unwrap_or_else(|e| e.into_inner());
    t.tick += 1;
    let tick = t.tick;
    let entry = t.by_hash.entry(hash).or_insert_with(|| Levels {
        levels: Vec::new(),
        used: tick,
    });
    entry.used = tick;
    if entry.levels.len() <= level {
        entry.levels.resize_with(level + 1, || None);
    }
    let tiles = entry.levels[level].get_or_insert_with(|| make_level(ctx, hash, &picture, level));
    // Seconds to level columns; the picture ends with its last whole frame.
    let col_secs = ColourSpectrum::column_secs() * (1u64 << level) as f64;
    let end = picture.frames as f64 * ColourSpectrum::column_secs() / col_secs;
    let mut meshes: Vec<Mesh> = tiles
        .iter()
        .map(|t| Mesh::with_texture(t.tex.id()))
        .collect();
    for &(x0, x1, f0, f1) in &strips {
        let (c0, c1) = (f0 / col_secs, f1 / col_secs);
        for (tile, mesh) in tiles.iter().zip(meshes.iter_mut()) {
            // The part of the strip this texture draws: where c is in from..to (and before
            // the picture's end).
            let lo = (tile.from as f64).max(0.0);
            let hi = (tile.to as f64).min(end);
            let (a, b) = (c0.min(c1).max(lo), c0.max(c1).min(hi));
            if b <= a {
                continue;
            }
            let x_at = |c: f64| {
                if (c1 - c0).abs() < 1e-12 {
                    x0
                } else {
                    x0 + ((c - c0) / (c1 - c0)) as f32 * (x1 - x0)
                }
            };
            let (ca, cb) = if c1 >= c0 { (a, b) } else { (b, a) };
            let (xa, xb) = (x_at(ca), x_at(cb));
            let span = (tile.end - tile.start) as f64;
            let u = |c: f64| ((c - tile.start as f64) / span) as f32;
            mesh.add_rect_with_uv(
                Rect::from_min_max(pos2(xa, area.top()), pos2(xb, area.bottom())),
                Rect::from_min_max(pos2(u(ca), 0.0), pos2(u(cb), 1.0)),
                tint,
            );
        }
    }
    for m in meshes {
        if !m.is_empty() {
            g.add(m);
        }
    }
    // Pictures not drawn for a while give their textures back.
    if t.by_hash.len() > KEEP_TEXTURES {
        let mut by_use: Vec<(u64, u64)> = t.by_hash.iter().map(|(h, l)| (l.used, *h)).collect();
        by_use.sort_unstable();
        let extra = by_use.len() - KEEP_TEXTURES;
        for (_, h) in by_use.into_iter().take(extra) {
            t.by_hash.remove(&h);
        }
    }
    true
}

/// The strips of screen from `left` to `right`, each with the file seconds at its two ends:
/// (x0, x1, f0, f1). Where the file stops playing inside a strip, the strip is cut there.
fn strips(
    left: f32,
    right: f32,
    file_secs: &impl Fn(f32) -> Option<f64>,
) -> Vec<(f32, f32, f64, f64)> {
    let mut out = Vec::new();
    let mut x = left;
    let mut f = file_secs(x);
    while x < right {
        let nx = (x + STRIP).min(right);
        let nf = file_secs(nx);
        match (f, nf) {
            (Some(a), Some(b)) => out.push((x, nx, a, b)),
            (Some(_), None) | (None, Some(_)) => {
                // Find where it starts or stops playing.
                let (mut lo, mut hi) = (x, nx);
                for _ in 0..20 {
                    let mid = (lo + hi) / 2.0;
                    if file_secs(mid).is_some() == f.is_some() {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                if let (Some(a), Some(b)) = (f, file_secs(lo)) {
                    out.push((x, lo, a, b));
                } else if let (Some(a), Some(b)) = (file_secs(hi), nf) {
                    out.push((hi, nx, a, b));
                }
            }
            (None, None) => {}
        }
        x = nx;
        f = nf;
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(crate) mod tests {
    use super::*;

    /// A test folder of its own.
    pub(crate) fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "prowave-colour-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sine(hz: f64, t: f64) -> f64 {
        (std::f64::consts::TAU * hz * t).sin()
    }

    /// Music-like test audio (made up, no recording): sections of `bars` bars at 120 bpm, each
    /// a different arrangement, as interleaved stereo at `rate`. Kinds: 0 intro (pad and
    /// hats), 1 verse (bass, kick, snare, hats), 2 chorus (everything, a bright lead, wide),
    /// 3 breakdown (low pad and sub only), 4 outro (pad fading out).
    pub(crate) fn song(rate: u32, sections: &[(usize, u32)]) -> Vec<f32> {
        let bar = 2.0;
        let total: f64 = sections.iter().map(|(_, b)| f64::from(*b) * bar).sum();
        let n = (total * f64::from(rate)) as usize;
        let mut out = Vec::with_capacity(n * 2);
        let mut noise = 0x1234_5678u32;
        let mut next = move || {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            f64::from(noise) / f64::from(u32::MAX) * 2.0 - 1.0
        };
        let roots = [55.0, 43.65, 49.0, 41.2];
        for i in 0..n {
            let t = i as f64 / f64::from(rate);
            let (mut at, mut kind, mut sec_t, mut sec_len) = (0.0, 0, 0.0, 1.0);
            for &(k, b) in sections {
                let len = f64::from(b) * bar;
                if t < at + len {
                    (kind, sec_t, sec_len) = (k, t - at, len);
                    break;
                }
                at += len;
            }
            let beat = (t * 2.0).fract();
            let root = roots[((t / bar) as usize) % 4];
            let eighth = (t * 4.0).fract();
            let hat = next() * (-eighth * 40.0).exp() * 0.12;
            let bt = beat * 0.5;
            let kick = sine(50.0 + 90.0 * (-bt * 60.0).exp(), bt) * (-bt * 18.0).exp() * 0.8;
            let snare_ph = ((t * 2.0 + 1.0) / 2.0).fract();
            let snare_on = if snare_ph < 0.5 { 0.25 } else { 0.0 };
            let snare = next() * (-snare_ph * 18.0).exp() * snare_on;
            let bass = sine(root, t) * 0.35 + sine(root * 2.0, t) * 0.1;
            let pad = [1.0, 1.26, 1.5, 2.0]
                .iter()
                .map(|m| sine(root * 4.0 * m, t))
                .sum::<f64>()
                * 0.06;
            let step = if ((t * 4.0) as u32).is_multiple_of(3) {
                1.5
            } else {
                1.0
            };
            let lead = sine(root * 16.0 * step, t) * 0.12 + next() * 0.02;
            let (l, r) = match kind {
                0 => (pad + hat, pad * 0.8 + hat * 0.6),
                1 => {
                    let m = bass + kick + snare + hat * 0.7;
                    (m, m)
                }
                2 => {
                    let m = bass + kick + snare;
                    let air = sine(root * 24.0, t) * 0.06;
                    (
                        m + hat + pad + lead,
                        m + hat * 1.2 + pad * 0.7 + lead * 0.5 + air,
                    )
                }
                3 => (
                    sine(root, t) * 0.4 + pad * 0.5,
                    sine(root, t) * 0.4 + pad * 0.4,
                ),
                _ => {
                    let fade = 1.0 - sec_t / sec_len;
                    (pad * fade, pad * fade * 0.9)
                }
            };
            out.push((l * 0.6) as f32);
            out.push((r * 0.6) as f32);
        }
        out
    }

    pub(crate) fn write(dir: &Path, name: &str, stereo: &[f32], rate: u32) -> AudioFile {
        let path = dir.join(name);
        prowave_engine::write_wav(&path, stereo, rate).unwrap();
        AudioFile::open(&path).unwrap()
    }

    /// Waits for the cache to have `file`'s picture.
    fn wait(cache: &Arc<Cache>, file: &AudioFile) -> (u64, Arc<ColourSpectrum>) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        loop {
            if let Some(p) = cache.get(None, file) {
                return p;
            }
            assert!(std::time::Instant::now() < deadline, "no picture made");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    fn a_picture_is_made_once_and_kept_by_the_files_content() {
        let dir = temp_dir("cache");
        let x = song(44_100, &[(1, 2), (2, 2)]);
        let a = write(&dir, "a.wav", &x, 44_100);
        let cache = Cache::new(dir.join("cache"));
        // Asked for: not there yet, and the drawing does not wait.
        assert!(cache.get(None, &a).is_none());
        let (hash, pic) = wait(&cache, &a);
        assert_eq!(cache.measured.load(Ordering::Relaxed), 1);
        assert_eq!(*pic, measure(&a).unwrap(), "the picture is the file's");
        // Asked again: the same picture, nothing measured.
        let (h2, p2) = cache.get(None, &a).unwrap();
        assert!(h2 == hash && Arc::ptr_eq(&p2, &pic));
        // The same sound under another name: found by its content, not measured.
        std::fs::copy(&a.path, dir.join("b.wav")).unwrap();
        let b = AudioFile::open(dir.join("b.wav")).unwrap();
        let (hb, pb) = wait(&cache, &b);
        assert!(hb == hash && Arc::ptr_eq(&pb, &pic));
        assert_eq!(cache.measured.load(Ordering::Relaxed), 1);
        // The next run (a new cache on the same folder): read from the disk, not measured.
        let next = Cache::new(dir.join("cache"));
        let (hn, pn) = wait(&next, &a);
        assert_eq!((hn, &*pn), (hash, &*pic));
        assert_eq!(next.measured.load(Ordering::Relaxed), 0);
        assert_eq!(next.from_disk.load(Ordering::Relaxed), 1);
        // Different sound: a different picture.
        let c = write(&dir, "c.wav", &song(44_100, &[(3, 2)]), 44_100);
        let (hc, pc) = wait(&next, &c);
        assert!(hc != hash && *pc != *pic);
        // A damaged cache file is measured again, not trusted.
        let f = dir.join("cache").join(format!("{hash:016x}.bin"));
        std::fs::write(&f, b"PWCS\x01\0\0\0junk").unwrap();
        let third = Cache::new(dir.join("cache"));
        let (_, p3) = wait(&third, &a);
        assert_eq!(*p3, *pic);
        assert_eq!(third.measured.load(Ordering::Relaxed), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cannot_be_read_is_asked_for_once() {
        let dir = temp_dir("missing");
        let cache = Cache::new(dir.join("cache"));
        let file = AudioFile {
            path: dir.join("gone.wav"),
            sample_rate: 48_000,
            frames: 48_000,
            channels: 2,
        };
        assert!(cache.get(None, &file).is_none());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !matches!(cache.lock().slots.values().next(), Some(Slot::Failed)) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(cache.get(None, &file).is_none());
        assert_eq!(cache.lock().queue.len(), 0, "not queued again");
    }

    #[test]
    fn cache_files_round_trip_and_reject_damage() {
        let p = ColourSpectrum {
            frames: 3,
            rows: 2,
            rgb: (0..18).collect(),
        };
        assert_eq!(decode(&encode(&p)), Some(p.clone()));
        let mut b = encode(&p);
        b.pop();
        assert_eq!(decode(&b), None);
        let mut b = encode(&p);
        b[4] = 9;
        assert_eq!(decode(&b), None, "another version");
    }

    #[test]
    fn strips_cover_what_plays_and_stop_where_it_stops() {
        // Plays from x = 10 to x = 50.5, one second per 10 points.
        let f = |x: f32| {
            (10.0..=50.5)
                .contains(&x)
                .then(|| f64::from(x - 10.0) / 10.0)
        };
        let s = strips(0.0, 100.0, &f);
        let first = s.first().unwrap();
        let last = s.last().unwrap();
        assert!(
            (first.0 - 10.0).abs() < 0.01 && first.2.abs() < 0.01,
            "{first:?}"
        );
        assert!(
            (last.1 - 50.5).abs() < 0.01 && (last.3 - 4.05).abs() < 0.01,
            "{last:?}"
        );
        for w in s.windows(2) {
            assert_eq!(w[0].1, w[1].0, "no gap");
        }
    }

    #[test]
    fn levels_average_columns() {
        let p = ColourSpectrum {
            frames: 5,
            rows: 1,
            rgb: vec![0, 0, 0, 100, 100, 100, 200, 0, 0, 0, 0, 0, 50, 50, 50],
        };
        assert_eq!(level_width(5, 1), 3);
        assert_eq!(level_pixel(&p, 1, 0, 0), Color32::from_rgb(50, 50, 50));
        assert_eq!(level_pixel(&p, 1, 0, 1), Color32::from_rgb(100, 0, 0));
        assert_eq!(level_pixel(&p, 1, 0, 2), Color32::from_rgb(50, 50, 50));
    }
}
