//! Optional plug-in ratings, types, and hardware emulation metadata for plug-in pickers.
//!
//! Reads `plugin-ratings.json` from Dōri's config directory (at most once per second, and
//! skipped in unit tests) with safe fallback to empty ratings on missing/malformed files.

use egui::Color32;
use std::collections::HashMap;
use std::sync::Arc;

/// Metadata for a single plug-in from `plugin-ratings.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rated {
    /// 1 to 5 in halves; `None` when not rated.
    pub rating: Option<f32>,
    pub why: String,
    /// The type folder it belongs to (`""` when unspecified).
    pub kind: String,
    /// Proper name of emulated hardware/instrument (`""` when none).
    pub emulates: String,
    /// Proper name and maker from catalog (`""` when unspecified).
    pub name: String,
    pub maker: String,
    /// Whether it is also listed in the Master Chain folder.
    pub master_chain: bool,
}

/// Parsed `plugin-ratings.json` table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ratings {
    by_id: HashMap<String, Rated>,
}

impl Ratings {
    /// Parses `{"plugins": {id: {rating, why, type, ...}}}` (invalid JSON returns empty).
    pub fn parse(text: &str) -> Self {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
            return Self::default();
        };
        let Some(map) = v.get("plugins").and_then(|p| p.as_object()) else {
            return Self::default();
        };
        let by_id = map
            .iter()
            .map(|(id, e)| {
                let s = |k: &str| {
                    e.get(k)
                        .and_then(|x| x.as_str())
                        .unwrap_or("")
                        .trim()
                        .to_string()
                };
                let rating = e
                    .get("rating")
                    .and_then(serde_json::Value::as_f64)
                    .filter(|r| r.is_finite())
                    .map(|r| r.clamp(1.0, 5.0) as f32);
                let rated = Rated {
                    rating,
                    why: s("why"),
                    kind: s("type"),
                    emulates: s("emulates"),
                    name: s("name"),
                    maker: s("maker"),
                    master_chain: e
                        .get("master_chain")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                };
                (id.clone(), rated)
            })
            .collect();
        Self { by_id }
    }

    /// Looks up `id` or its alternate format `alt`.
    pub fn of(&self, id: &str, alt: Option<&str>) -> Option<&Rated> {
        self.by_id
            .get(id)
            .or_else(|| alt.and_then(|a| self.by_id.get(a)))
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    /// Whether no plug-in ratings are loaded.
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }
}

/// Colour of a rating chip interpolated in OKLab across Dōri's dark card to amber accent.
pub fn color(rating: f32) -> Color32 {
    let t = (((rating.clamp(1.0, 5.0) - 1.0) * 2.0).round() / 8.0).clamp(0.0, 1.0);
    let (a, b) = (
        oklab(crate::theme::CARD_BG),
        oklab(crate::theme::ACCENT_AMBER),
    );
    from_oklab([0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t))
}

/// Contrasting text colour (black or white) for a chip of colour `fill`.
pub fn chip_text(fill: Color32) -> Color32 {
    if contrast(fill, Color32::BLACK) >= contrast(fill, Color32::WHITE) {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

fn linear(v: u8) -> f32 {
    let v = f32::from(v) / 255.0;
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

fn encode(v: f32) -> u8 {
    let v = v.clamp(0.0, 1.0);
    let v = if v <= 0.003_130_8 {
        12.92 * v
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round() as u8
}

fn luminance(c: Color32) -> f32 {
    0.2126 * linear(c.r()) + 0.7152 * linear(c.g()) + 0.0722 * linear(c.b())
}

/// WCAG contrast ratio of two colours.
pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

fn oklab(c: Color32) -> [f32; 3] {
    let (r, g, b) = (linear(c.r()), linear(c.g()), linear(c.b()));
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

fn from_oklab([l, a, b]: [f32; 3]) -> Color32 {
    let l_ = (l + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m_ = (l - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s_ = (l - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    Color32::from_rgb(
        encode(4.076_741_7 * l_ - 3.307_711_6 * m_ + 0.230_969_94 * s_),
        encode(-1.268_438 * l_ + 2.609_757_4 * m_ - 0.341_319_38 * s_),
        encode(-0.004_196_086_3 * l_ - 0.703_418_6 * m_ + 1.707_614_7 * s_),
    )
}

/// Formats a rating (`"4.5"`, `"3"`).
pub fn label(rating: f32) -> String {
    if rating.fract() == 0.0 {
        format!("{rating:.0}")
    } else {
        format!("{rating:.1}")
    }
}

/// Display name and secondary label for a plug-in, preferring emulated hardware name if present.
pub fn shown(name: &str, maker: &str, rated: Option<&Rated>) -> (String, String) {
    match rated.filter(|r| !r.emulates.is_empty()) {
        Some(r) => {
            let own = if r.name.is_empty() { name } else { &r.name };
            (r.emulates.clone(), own.to_string())
        }
        None => (name.to_string(), maker.to_string()),
    }
}

/// Whether `query` matches the plug-in's name, maker, or emulated hardware name.
pub fn matches(query: &str, name: &str, maker: &str, rated: Option<&Rated>) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    let mut hay = vec![name, maker];
    if let Some(r) = rated {
        hay.extend([r.emulates.as_str(), r.name.as_str(), r.maker.as_str()]);
    }
    hay.iter().any(|h| h.to_lowercase().contains(&q))
}

static CACHE: std::sync::Mutex<Option<Cache>> = std::sync::Mutex::new(None);

struct Cache {
    looked: std::time::Instant,
    modified: Option<std::time::SystemTime>,
    ratings: Arc<Ratings>,
}

/// Returns the cached ratings from `plugin-ratings.json` (re-checked at most once a second).
pub fn ratings() -> Arc<Ratings> {
    if cfg!(test) {
        return Arc::new(Ratings::default());
    }
    let Ok(mut c) = CACHE.lock() else {
        return Arc::new(Ratings::default());
    };
    if let Some(now) = c.as_ref() {
        if now.looked.elapsed() < std::time::Duration::from_secs(1) {
            return now.ratings.clone();
        }
    }
    let file = crate::recent_files::config_dir().map(|d| d.join("plugin-ratings.json"));
    let modified = file
        .as_ref()
        .and_then(|f| std::fs::metadata(f).ok())
        .and_then(|m| m.modified().ok());
    let ratings = match c.as_ref() {
        Some(old) if old.modified == modified => old.ratings.clone(),
        _ => Arc::new(
            file.and_then(|f| std::fs::read_to_string(f).ok())
                .map(|t| Ratings::parse(&t))
                .unwrap_or_default(),
        ),
    };
    *c = Some(Cache {
        looked: std::time::Instant::now(),
        modified,
        ratings: ratings.clone(),
    });
    ratings
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"{"plugins": {
        "aufx:Pro3:FbFt": {"rating": 5, "why": "Standard EQ.", "type": "EQ"},
        "vst3:AB": {"rating": 2.5, "why": "Overlaps.", "type": "EQ"},
        "aufx:dely:Xyzw": {"rating": 3, "why": "", "type": "Delay"},
        "builtin:Space D": {"rating": 4, "why": "Built-in.", "type": "Reverb"},
        "aufx:odd1:Xyzw": {"rating": 1, "type": "other"}
    }}"#;

    #[test]
    fn parses_ratings_and_handles_invalid_json() {
        let r = Ratings::parse(FILE);
        assert_eq!(r.len(), 5);
        assert_eq!(
            r.of("aufx:Ch12:Xyzw", Some("vst3:AB"))
                .and_then(|x| x.rating),
            Some(2.5)
        );
        assert_eq!(Ratings::parse("not json").len(), 0);
        assert!(Ratings::parse(r#"{"plugins":{"x":{"rating":NaN}}}"#)
            .of("x", None)
            .is_none());
    }

    #[test]
    fn emulations_match_and_format_with_contrast() {
        let r = Ratings::parse(
            r#"{"plugins": {
            "aumu:JC33:Xyzw": {"rating": 4, "why": "Good 303.", "type": "Synth",
                "emulates": "Roland TB-303", "name": "JC303", "maker": "midilab"}
        }}"#,
        );
        let jc = r.of("aumu:JC33:Xyzw", None);
        assert_eq!(
            shown("JC303", "midilab", jc),
            ("Roland TB-303".to_string(), "JC303".to_string())
        );
        assert!(matches("tb-303", "JC303", "midilab", jc));
        assert!(matches("jc3", "JC303", "midilab", jc));
        assert!(matches("MIDILAB", "JC303", "midilab", jc));
        assert!(!matches("808", "JC303", "midilab", jc));

        let c = color(5.0);
        assert!(contrast(c, chip_text(c)) >= 4.5);
    }
}
