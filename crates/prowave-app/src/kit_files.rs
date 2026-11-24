//! Reads one drum kit from its folder: every audio file in it with its class, and, when the
//! kit comes with a definition (an SFZ file, a Hydrogen `drumkit.xml` or a DrumGizmo kit), the
//! instruments that definition names: one sample each, the loudest velocity layer, with the
//! class its name or its General MIDI note gives. [`Kit::pads`] is what a kit load puts on the
//! Channel Rack. Nothing in the folder is changed. Runs on a background thread.
use crate::kit_classify::{classify, classify_label, gm_class, natural_cmp, DrumClass};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Most audio files read from one kit.
pub const MAX_KIT_FILES: usize = 10_000;
/// A definition file larger than this is not read.
const MAX_DEFINITION_BYTES: u64 = 8 * 1024 * 1024;

/// How a kit says what its samples are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KitKind {
    /// Loose audio files: the names say it.
    Files,
    Sfz,
    Hydrogen,
    DrumGizmo,
}

impl KitKind {
    pub fn id(self) -> &'static str {
        match self {
            KitKind::Files => "files",
            KitKind::Sfz => "sfz",
            KitKind::Hydrogen => "hydrogen",
            KitKind::DrumGizmo => "drumgizmo",
        }
    }
}

/// One audio file of a kit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KitSample {
    /// Its path inside the kit folder, with `/`.
    pub rel: String,
    /// Its file name without the extension.
    pub name: String,
    pub class: DrumClass,
}

/// A kit read from its folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kit {
    pub root: PathBuf,
    pub kind: KitKind,
    /// Every audio file, sorted by path.
    pub samples: Vec<KitSample>,
    /// The channels a kit load fills: indices into `samples`, `None` an empty channel.
    pub pads: Vec<Option<usize>>,
    /// More than [`MAX_KIT_FILES`] audio files were there; the rest were left out.
    pub cut: bool,
}

impl Kit {
    pub fn path_of(&self, i: usize) -> PathBuf {
        self.root.join(&self.samples[i].rel)
    }
}

/// One instrument of a definition: its sample (inside the kit, with `/`) and its class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instrument {
    pub rel: String,
    pub class: Option<DrumClass>,
    pub note: Option<u8>,
}

fn is_audio(name: &str) -> bool {
    crate::import::is_audio_path(Path::new(name))
}

/// Every audio file and every `.sfz` and `.xml` file under `root` (paths inside it, with `/`),
/// sorted; hidden files and `__MACOSX` are skipped. It ends at [`MAX_KIT_FILES`] audio files, and
/// when `stop` is set (the kit is no longer wanted: a slow disk is not read for nothing).
fn walk(root: &Path, stop: &AtomicBool) -> (Vec<String>, Vec<String>, bool) {
    let mut audio = Vec::new();
    let mut defs = Vec::new();
    let mut cut = false;
    let mut stack = vec![(root.to_path_buf(), String::new(), 0usize)];
    'dirs: while let Some((dir, prefix, depth)) = stack.pop() {
        let Ok(read) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in read.flatten() {
            if stop.load(Ordering::Relaxed) {
                break 'dirs;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name == "__MACOSX" {
                continue;
            }
            let rel = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let is_dir = e.file_type().is_ok_and(|t| t.is_dir());
            if is_dir {
                if depth < 12 {
                    stack.push((e.path(), rel, depth + 1));
                }
            } else if is_audio(&name) {
                if audio.len() >= MAX_KIT_FILES {
                    cut = true;
                    break 'dirs;
                }
                audio.push(rel);
            } else {
                let low = name.to_ascii_lowercase();
                if low.ends_with(".sfz") || low.ends_with(".xml") {
                    defs.push(rel);
                }
            }
        }
    }
    audio.sort_by(|a, b| natural_cmp(a, b));
    defs.sort_by(|a, b| {
        a.matches('/')
            .count()
            .cmp(&b.matches('/').count())
            .then_with(|| natural_cmp(a, b))
    });
    (audio, defs, cut)
}

fn read_text(path: &Path) -> Option<String> {
    let len = std::fs::metadata(path).ok()?.len();
    if len > MAX_DEFINITION_BYTES {
        return None;
    }
    Some(String::from_utf8_lossy(&std::fs::read(path).ok()?).into_owned())
}

/// `dir` (inside the kit) joined with `file` as a definition writes it (either slash, `./`,
/// `../`), without leaving the kit.
fn join_rel(dir: &str, file: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    let file = file.replace('\\', "/");
    for p in file.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            p => parts.push(p),
        }
    }
    parts.join("/")
}

fn dir_of(rel: &str) -> &str {
    rel.rsplit_once('/').map_or("", |(d, _)| d)
}

// ------------------------------------------------------------------------------------ SFZ

/// A key as SFZ writes it: a number, or a note name (c4 is 60).
fn sfz_key(v: &str) -> Option<u8> {
    let v = v.trim();
    if let Ok(n) = v.parse::<u8>() {
        return (n <= 127).then_some(n);
    }
    let b = v.as_bytes();
    let pc = match b.first()?.to_ascii_lowercase() {
        b'c' => 0,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return None,
    };
    let (shift, rest) = match b.get(1)? {
        b'#' => (1, &v[2..]),
        b'b' => (-1, &v[2..]),
        _ => (0, &v[1..]),
    };
    let oct: i32 = rest.parse().ok()?;
    u8::try_from((oct + 1) * 12 + pc + shift)
        .ok()
        .filter(|n| *n <= 127)
}

/// The opcodes of one line piece (`key=36 sample=Kick 1.wav hivel=127`): a value runs to the
/// next ` name=`, so a sample's name can hold spaces.
fn sfz_opcodes(text: &str) -> Vec<(String, String)> {
    let mut starts: Vec<(usize, usize)> = Vec::new();
    let b = text.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if *c != b'=' {
            continue;
        }
        let mut s = i;
        while s > 0 && (b[s - 1].is_ascii_alphanumeric() || b[s - 1] == b'_') {
            s -= 1;
        }
        if s < i && (s == 0 || b[s - 1].is_ascii_whitespace()) {
            starts.push((s, i));
        }
    }
    let mut out = Vec::new();
    for (k, (s, eq)) in starts.iter().enumerate() {
        let end = starts.get(k + 1).map_or(text.len(), |n| n.0);
        out.push((
            text[*s..*eq].to_ascii_lowercase(),
            text[eq + 1..end].trim().to_string(),
        ));
    }
    out
}

/// The instruments of an SFZ file `rel` (inside the kit): for each key the region with the
/// highest velocity range. A group's comment or label names the instrument.
pub fn parse_sfz(text: &str, rel: &str) -> Vec<Instrument> {
    #[derive(Clone, Default)]
    struct Scope {
        ops: HashMap<String, String>,
        label: String,
    }
    let base = dir_of(rel);
    let mut defines: Vec<(String, String)> = Vec::new();
    let mut default_path = String::new();
    let (mut global, mut master, mut group) =
        (Scope::default(), Scope::default(), Scope::default());
    let mut region: Option<Scope> = None;
    let mut header = String::new();
    let mut comment = String::new();
    // (key, hivel, order) -> instrument
    let mut found: Vec<(Option<u8>, u32, Instrument)> = Vec::new();
    let mut close = |r: Option<Scope>, default_path: &str| {
        let Some(r) = r else { return };
        let Some(sample) = r.ops.get("sample").filter(|s| !s.is_empty()) else {
            return;
        };
        if sample.starts_with('*') {
            return;
        }
        let key = ["key", "lokey", "pitch_keycenter", "hikey"]
            .iter()
            .find_map(|k| r.ops.get(*k).and_then(|v| sfz_key(v)));
        let hivel = r
            .ops
            .get("hivel")
            .and_then(|v| v.trim().parse::<u32>().ok())
            .unwrap_or(127);
        let file = format!("{default_path}{sample}");
        let rel = join_rel(base, &file);
        let class = classify_label(&r.label)
            .or_else(|| Some(classify(&rel)).filter(|c| *c != DrumClass::Other))
            .or_else(|| key.and_then(gm_class));
        found.push((
            key,
            hivel,
            Instrument {
                rel,
                class,
                note: key,
            },
        ));
    };
    for raw in text.lines() {
        let mut line = raw.to_string();
        if let Some(i) = line.find("//") {
            let c = line[i + 2..].trim().trim_start_matches('+').trim();
            if !c.is_empty() && !c.contains(':') {
                comment = c.to_string();
            }
            line.truncate(i);
        }
        // A comment inside a group names it ("<group>\n // Bass Drum 1").
        if header == "group" && group.label.is_empty() && !comment.is_empty() {
            group.label = std::mem::take(&mut comment);
        }
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("#define") {
            let mut it = rest.split_whitespace();
            if let (Some(name), Some(value)) = (it.next(), it.next()) {
                defines.push((name.to_string(), value.to_string()));
            }
            continue;
        }
        if t.starts_with('#') || t.is_empty() {
            continue;
        }
        for (name, value) in &defines {
            if line.contains(name.as_str()) {
                line = line.replace(name.as_str(), value);
            }
        }
        // Cut the line at its headers: text before the first header belongs to the open one.
        let mut rest = line.as_str();
        loop {
            let (before, next) = match rest.find('<') {
                Some(i) => (&rest[..i], Some(&rest[i..])),
                None => (rest, None),
            };
            for (k, v) in sfz_opcodes(before) {
                let scope = match header.as_str() {
                    "region" => region.as_mut(),
                    "group" => Some(&mut group),
                    "master" => Some(&mut master),
                    "global" => Some(&mut global),
                    "control" => {
                        if k == "default_path" {
                            default_path = v.replace('\\', "/");
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(s) = scope {
                    if k == "group_label" || k == "region_label" || k == "master_label" {
                        s.label = v;
                    } else {
                        s.ops.insert(k, v);
                    }
                }
            }
            let Some(next) = next else { break };
            let Some(end) = next.find('>') else { break };
            let name = next[1..end].trim().to_ascii_lowercase();
            if matches!(
                name.as_str(),
                "region" | "group" | "master" | "global" | "control"
            ) {
                close(region.take(), &default_path);
            }
            match name.as_str() {
                "global" => {
                    global = Scope::default();
                    master = Scope::default();
                    group = Scope::default();
                }
                "master" => {
                    master = Scope {
                        ops: global.ops.clone(),
                        label: std::mem::take(&mut comment),
                    };
                    group = master.clone();
                }
                "group" => {
                    let mut ops = global.ops.clone();
                    ops.extend(master.ops.clone());
                    let label = std::mem::take(&mut comment);
                    group = Scope {
                        ops,
                        label: if label.is_empty() {
                            master.label.clone()
                        } else {
                            label
                        },
                    };
                }
                "region" => {
                    let mut ops = global.ops.clone();
                    ops.extend(master.ops.clone());
                    ops.extend(group.ops.clone());
                    region = Some(Scope {
                        ops,
                        label: group.label.clone(),
                    });
                }
                _ => {}
            }
            header = name;
            rest = &next[end + 1..];
        }
    }
    close(region.take(), &default_path);
    // One instrument a key: the loudest layer, the first of several.
    let mut out: Vec<(Option<u8>, u32, Instrument)> = Vec::new();
    for f in found {
        match out.iter_mut().find(|o| f.0.is_some() && o.0 == f.0) {
            Some(o) => {
                if f.1 > o.1 {
                    *o = f;
                }
            }
            None => {
                if !out.iter().any(|o| o.2.rel == f.2.rel) {
                    out.push(f);
                }
            }
        }
    }
    out.sort_by_key(|o| o.0.unwrap_or(255));
    out.into_iter().map(|o| o.2).collect()
}

// ------------------------------------------------------------------------------------ XML

/// The text inside each `<tag ...>` to `</tag>` of `text`, with the attributes of its start.
fn blocks<'a>(text: &'a str, tag: &str) -> Vec<(&'a str, &'a str)> {
    let open = format!("<{tag}");
    let close = format!("</{tag}>");
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = text[at..].find(&open) {
        let start = at + i;
        let after = start + open.len();
        // "<instrumentList" is not "<instrument".
        let boundary = text[after..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c == '/' || c.is_whitespace());
        let Some(gt) = text[after..].find('>') else {
            break;
        };
        let head_end = after + gt;
        if !boundary {
            at = after;
            continue;
        }
        let attrs = text[after..head_end].trim_end_matches('/');
        if text[after..head_end].ends_with('/') {
            out.push((attrs, ""));
            at = head_end + 1;
            continue;
        }
        match text[head_end + 1..].find(&close) {
            Some(c) => {
                out.push((attrs, &text[head_end + 1..head_end + 1 + c]));
                at = head_end + 1 + c + close.len();
            }
            None => break,
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// The text of the first `<tag>` in `text`.
fn text_of(text: &str, tag: &str) -> Option<String> {
    blocks(text, tag)
        .first()
        .map(|(_, inner)| unescape(inner.trim()))
}

/// The value of attribute `name` in the attributes of a start tag.
fn attr(attrs: &str, name: &str) -> Option<String> {
    let mut at = 0;
    while let Some(i) = attrs[at..].find(name) {
        let s = at + i;
        let before_ok = s == 0 || attrs.as_bytes()[s - 1].is_ascii_whitespace();
        let rest = attrs[s + name.len()..].trim_start();
        if before_ok && rest.starts_with('=') {
            let v = rest[1..].trim_start();
            let q = v.chars().next()?;
            if q == '"' || q == '\'' {
                let end = v[1..].find(q)?;
                return Some(unescape(&v[1..1 + end]));
            }
        }
        at = s + name.len();
    }
    None
}

/// The instruments of a Hydrogen `drumkit.xml` at `rel`: each instrument's layer with the
/// highest velocity (or its one file, in the old format).
pub fn parse_hydrogen(text: &str, rel: &str) -> Vec<Instrument> {
    let base = dir_of(rel);
    let mut out = Vec::new();
    for (_, inst) in blocks(text, "instrument") {
        let layers = blocks(inst, "layer");
        let file = if layers.is_empty() {
            text_of(inst, "filename")
        } else {
            let mut best: Option<(f32, String)> = None;
            for (_, l) in &layers {
                let Some(f) = text_of(l, "filename") else {
                    continue;
                };
                let max = text_of(l, "max")
                    .and_then(|m| m.parse::<f32>().ok())
                    .unwrap_or(1.0);
                if best.as_ref().is_none_or(|b| max > b.0) {
                    best = Some((max, f));
                }
            }
            best.map(|b| b.1)
        };
        let Some(file) = file.filter(|f| !f.is_empty()) else {
            continue;
        };
        // The name comes before the layers; a layer has no <name>.
        let head = inst.split("<layer").next().unwrap_or(inst);
        let head = head.split("<instrumentComponent").next().unwrap_or(head);
        let name = text_of(head, "name").unwrap_or_default();
        let note = text_of(head, "midiOutNote")
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| *n <= 127);
        let rel = join_rel(base, &file);
        let class = classify_label(&name)
            .or_else(|| Some(classify(&rel)).filter(|c| *c != DrumClass::Other))
            .or_else(|| note.and_then(gm_class));
        out.push(Instrument { rel, class, note });
    }
    out
}

/// Whether `text` is the top file of a DrumGizmo kit.
fn is_drumgizmo(text: &str) -> bool {
    text.contains("<drumkit")
        && blocks(text, "instrument")
            .iter()
            .any(|(a, _)| attr(a, "file").is_some())
}

/// The instruments of a DrumGizmo kit whose top file is `rel`: for each, the sample with the
/// most power in its own file (read through `read`, which is given a path inside the kit).
pub fn parse_drumgizmo(
    text: &str,
    rel: &str,
    read: &dyn Fn(&str) -> Option<String>,
) -> Vec<Instrument> {
    let base = dir_of(rel);
    let mut out = Vec::new();
    for (a, _) in blocks(text, "instrument") {
        let (Some(name), Some(file)) = (attr(a, "name"), attr(a, "file")) else {
            continue;
        };
        let inst_rel = join_rel(base, &file);
        let Some(inst) = read(&inst_rel) else {
            continue;
        };
        let mut best: Option<(f32, String)> = None;
        for (sa, body) in blocks(&inst, "sample") {
            let Some(audio) = blocks(body, "audiofile")
                .iter()
                .find_map(|(aa, _)| attr(aa, "file"))
            else {
                continue;
            };
            let power = attr(sa, "power")
                .and_then(|p| p.parse::<f32>().ok())
                .unwrap_or(0.0);
            if best.as_ref().is_none_or(|b| power >= b.0) {
                best = Some((power, audio));
            }
        }
        let Some((_, audio)) = best else { continue };
        let rel = join_rel(dir_of(&inst_rel), &audio);
        let class = classify_label(&name)
            .or_else(|| Some(classify(&rel)).filter(|c| *c != DrumClass::Other));
        out.push(Instrument {
            rel,
            class,
            note: None,
        });
    }
    out
}

// ------------------------------------------------------------------------------------ the kit

/// Finds a sample a definition names among the kit's files (the case of letters may differ).
fn find_sample(samples: &[KitSample], rel: &str) -> Option<usize> {
    samples
        .iter()
        .position(|s| s.rel == rel)
        .or_else(|| samples.iter().position(|s| s.rel.eq_ignore_ascii_case(rel)))
}

/// Reads the kit in `root`.
pub fn read_kit(root: &Path) -> Kit {
    read_kit_until(root, &AtomicBool::new(false))
}

/// [`read_kit`], given up when `stop` is set (what it returns then is not the whole kit).
pub fn read_kit_until(root: &Path, stop: &AtomicBool) -> Kit {
    let (audio, defs, cut) = walk(root, stop);
    let mut samples: Vec<KitSample> = audio
        .into_iter()
        .map(|rel| {
            let file = rel.rsplit('/').next().unwrap_or(&rel);
            let name = file.rsplit_once('.').map_or(file, |(s, _)| s).to_string();
            let class = classify(&rel);
            KitSample { rel, name, class }
        })
        .collect();
    // The first definition that names samples that are there.
    let mut kind = KitKind::Files;
    let mut instruments: Vec<(usize, Option<DrumClass>)> = Vec::new();
    for def in defs.iter().take(64) {
        let Some(text) = read_text(&root.join(def)) else {
            continue;
        };
        let low = def.to_ascii_lowercase();
        let (k, list) = if low.ends_with(".sfz") {
            (KitKind::Sfz, parse_sfz(&text, def))
        } else if text.contains("<instrumentList") {
            (KitKind::Hydrogen, parse_hydrogen(&text, def))
        } else if is_drumgizmo(&text) {
            let read = |rel: &str| read_text(&root.join(rel));
            (KitKind::DrumGizmo, parse_drumgizmo(&text, def, &read))
        } else {
            continue;
        };
        let here: Vec<(usize, Option<DrumClass>)> = list
            .iter()
            .filter_map(|i| find_sample(&samples, &i.rel).map(|s| (s, i.class)))
            .collect();
        if !here.is_empty() {
            kind = k;
            instruments = here;
            break;
        }
    }
    // A definition's word on a sample's class is better than its file name's.
    for (s, class) in &instruments {
        if let Some(c) = class {
            samples[*s].class = *c;
        }
    }
    let pads = if instruments.is_empty() {
        let classes: Vec<DrumClass> = samples.iter().map(|s| s.class).collect();
        crate::kit_classify::pads(&classes)
    } else {
        // One sample an instrument: the other velocity layers stay off the channels.
        let classes: Vec<DrumClass> = instruments.iter().map(|(s, _)| samples[*s].class).collect();
        crate::kit_classify::pads(&classes)
            .into_iter()
            .map(|p| p.map(|i| instruments[i].0))
            .collect()
    };
    Kit {
        root: root.to_path_buf(),
        kind,
        samples,
        pads,
        cut,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use DrumClass::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("prowave-kit-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(root: &Path, rel: &str, text: &str) {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// The head of FreePats' SynthesizerPercussion, as it is in the library.
    const FREEPATS: &str = "//+ Name: Synthesizer percussion\n\n<global>\n //+ Instrument: Synthesizer percussion\n ampeg_release=100\n loop_mode=no_loop\n\n<group>\n // Bass Drum 1\n key=48\n<region>\n sample=samples/Kick04.wav\n\n<group>\n // Snare Drum 1\n key=50\n<region>\n sample=samples/Snare09.wav\n\n<group>\n // Open Hi-Hat\n key=54\n volume=-2\n<region>\n sample=samples/OpenHiHat02-01.wav\n\n<group>\n // Cymbal 1\n //+ RandomRegion: Yes\n key=55\n<region>\n lorand=0 hirand=0.333333\n sample=samples/Cymbal01-01.wav\n<region>\n lorand=0.333333 hirand=0.666666\n sample=samples/Cymbal01-02.wav\n";

    #[test]
    fn an_sfz_file_gives_one_instrument_a_key() {
        let got = parse_sfz(FREEPATS, "pack/kit.sfz");
        let want = [
            ("pack/samples/Kick04.wav", Kick, 48),
            ("pack/samples/Snare09.wav", Snare, 50),
            ("pack/samples/OpenHiHat02-01.wav", OpenHat, 54),
            ("pack/samples/Cymbal01-01.wav", Crash, 55),
        ];
        assert_eq!(got.len(), want.len(), "{got:?}");
        for (g, (rel, class, note)) in got.iter().zip(want) {
            assert_eq!(
                (g.rel.as_str(), g.class, g.note),
                (rel, Some(class), Some(note))
            );
        }
    }

    #[test]
    fn sfz_velocity_layers_names_with_spaces_and_note_names() {
        let text = "<control> default_path=..\\Samples\\\n#define $KICK 36\n\
            <group> key=$KICK\n\
            <region> sample=Drum A soft.wav lovel=0 hivel=60\n\
            <region> sample=Drum A hard.wav lovel=61 hivel=127\n\
            <group> group_label=Thing\n<region> key=d2 sample=x1.wav\n\
            <region> key=F#2 sample=x2.wav <region> key=20 sample=x3.wav";
        let got = parse_sfz(text, "kit/Programs/a.sfz");
        assert_eq!(got.len(), 4, "{got:?}");
        assert_eq!(got[0].rel, "kit/Samples/x3.wav");
        assert_eq!(got[0].class, None, "no name and no drum note");
        assert_eq!(got[1].rel, "kit/Samples/Drum A hard.wav");
        assert_eq!(got[1].class, Some(Kick), "note 36");
        assert_eq!((got[2].note, got[2].class), (Some(38), Some(Snare)));
        assert_eq!((got[3].note, got[3].class), (Some(42), Some(ClosedHat)));
        assert_eq!(sfz_key("c4"), Some(60));
        assert_eq!(sfz_key("a#-1"), Some(10));
        assert_eq!(sfz_key("x"), None);
    }

    const HYDROGEN: &str = r#"<?xml version="1.0"?>
<drumkit_info><name>Test &amp; Kit</name>
<instrumentList>
 <instrument><id>0</id><name>Kick</name><volume>1</volume>
  <instrumentComponent><component_id>0</component_id>
   <layer><filename>kick-soft.flac</filename><min>0</min><max>0.5</max></layer>
   <layer><filename>kick-hard.flac</filename><min>0.5</min><max>1</max></layer>
  </instrumentComponent></instrument>
 <instrument><id>1</id><filename>sn.wav</filename><name>Snare Rock</name></instrument>
 <instrument><id>2</id><name>Thing</name><midiOutNote>46</midiOutNote>
  <layer><filename>a.wav</filename><min>0</min><max>1</max></layer></instrument>
 <instrument><id>3</id><name>Empty</name></instrument>
</instrumentList></drumkit_info>"#;

    #[test]
    fn a_hydrogen_kit_gives_its_loudest_layers() {
        let got = parse_hydrogen(HYDROGEN, "drumkit.xml");
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(
            (got[0].rel.as_str(), got[0].class),
            ("kick-hard.flac", Some(Kick))
        );
        assert_eq!((got[1].rel.as_str(), got[1].class), ("sn.wav", Some(Snare)));
        assert_eq!(
            (got[2].rel.as_str(), got[2].class),
            ("a.wav", Some(OpenHat))
        );
    }

    #[test]
    fn a_drumgizmo_kit_gives_each_instruments_strongest_sample() {
        let top = r#"<drumkit name="K" description="d"><channels><channel name="AmbL"/></channels>
<instruments>
 <instrument name="Snare" group="snare" file="Snare/Snare.xml"><channelmap in="AmbL" out="AmbL"/></instrument>
 <instrument name="KDrum" file="Kick/Kick.xml"/>
 <instrument name="Gone" file="Gone/Gone.xml"/>
</instruments></drumkit>"#;
        let snare = r#"<instrument version="2.0" name="Snare"><samples>
 <sample name="Snare-1" power="0.1"><audiofile channel="AmbL" file="samples/1-Snare.wav" filechannel="1"/></sample>
 <sample name="Snare-2" power="0.9"><audiofile channel="AmbL" file="samples/2-Snare.wav" filechannel="1"/><audiofile channel="AmbR" file="samples/2-Snare.wav" filechannel="2"/></sample>
</samples></instrument>"#;
        let kick = r#"<instrument name="KDrum"><samples><sample name="k"><audiofile file="samples/kick.wav"/></sample></samples></instrument>"#;
        assert!(is_drumgizmo(top));
        assert!(!is_drumgizmo(HYDROGEN));
        let read = |rel: &str| match rel {
            "kit/Snare/Snare.xml" => Some(snare.to_string()),
            "kit/Kick/Kick.xml" => Some(kick.to_string()),
            _ => None,
        };
        let got = parse_drumgizmo(top, "kit/kit.xml", &read);
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(
            (got[0].rel.as_str(), got[0].class),
            ("kit/Snare/samples/2-Snare.wav", Some(Snare))
        );
        assert_eq!(
            (got[1].rel.as_str(), got[1].class),
            ("kit/Kick/samples/kick.wav", Some(Kick))
        );
    }

    #[test]
    fn a_folder_of_loose_files_is_read_sorted_and_mapped() {
        let dir = temp("loose");
        for f in [
            "Kicks/Kick 10.wav",
            "Kicks/Kick 2.wav",
            "Snares/SD_a.WAV",
            "hats/chh.aif",
            "hats/ohh.flac",
            "Loops/120bpm/beat.wav",
            "readme.txt",
            ".hidden/kick.wav",
            "__MACOSX/Kicks/._Kick 2.wav",
        ] {
            touch(&dir, f, "");
        }
        let kit = read_kit(&dir);
        assert_eq!(kit.kind, KitKind::Files);
        let rels: Vec<&str> = kit.samples.iter().map(|s| s.rel.as_str()).collect();
        assert_eq!(
            rels,
            [
                "hats/chh.aif",
                "hats/ohh.flac",
                "Kicks/Kick 2.wav",
                "Kicks/Kick 10.wav",
                "Loops/120bpm/beat.wav",
                "Snares/SD_a.WAV"
            ]
        );
        // Kick, Snare, the second kick where a clap would be, Closed Hat, Open Hat.
        assert_eq!(
            kit.pads,
            [Some(2), Some(5), Some(3), Some(0), Some(1)],
            "{:?}",
            kit.samples
        );
        assert_eq!(kit.path_of(2), dir.join("Kicks/Kick 2.wav"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_kit_with_a_definition_puts_one_layer_of_each_instrument_on_the_channels() {
        let dir = temp("def");
        touch(&dir, "drumkit.xml", HYDROGEN);
        for f in [
            "kick-soft.flac",
            "kick-hard.flac",
            "sn.wav",
            "a.wav",
            "extra.wav",
        ] {
            touch(&dir, f, "");
        }
        let kit = read_kit(&dir);
        assert_eq!(kit.kind, KitKind::Hydrogen);
        let at = |n: &str| kit.samples.iter().position(|s| s.rel == n).unwrap();
        assert_eq!(
            kit.samples[at("a.wav")].class,
            OpenHat,
            "the definition says so"
        );
        // Kick, Snare, no clap, no closed hat, the open hat on its own row.
        assert_eq!(
            kit.pads,
            [
                Some(at("kick-hard.flac")),
                Some(at("sn.wav")),
                None,
                None,
                Some(at("a.wav"))
            ]
        );
        assert_eq!(kit.samples.len(), 5, "every file stays in the list");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_definition_that_names_no_file_here_is_passed_over() {
        let dir = temp("nodef");
        touch(&dir, "a.sfz", "<region> sample=missing.wav key=36");
        touch(&dir, "other.xml", "<foo/>");
        touch(&dir, "snare.wav", "");
        let kit = read_kit(&dir);
        assert_eq!(kit.kind, KitKind::Files);
        assert_eq!(kit.pads, [None, Some(0)]);
        let empty = read_kit(&dir.join("nothing-here"));
        assert!(empty.samples.is_empty() && empty.pads.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
