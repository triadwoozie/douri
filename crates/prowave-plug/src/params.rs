// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Pro Wave contributors.

//! The host's parameters, made from the registry's table: one whole-number parameter per entry,
//! with its range and default, its value text (a choice's name, "-3.5dB", "20ms"...) and the
//! reverse, so a value typed in a host's field is understood. The parameter ids are "p" and
//! the index ("p0", "p1"...): the index is also how a Pro Wave project stores the values, so
//! the order never changes. The values are saved with the host's project by nih-plug.

use crate::pages::{group, Page};
use nih_plug::prelude::*;
use nih_plug_egui::EguiState;
use prowave_engine::NativeFxInfo;
use std::sync::Arc;

pub struct PlugParams {
    pub info: &'static NativeFxInfo,
    pub instrument: bool,
    pub pages: Vec<Page>,
    pub editor_state: Arc<EguiState>,
    /// One per registry entry, in its order.
    pub values: Vec<IntParam>,
    ids: Vec<String>,
    groups: Vec<String>,
}

/// The number at the start of `text` ("-3.5dB" gives -3.5), if any.
fn leading_number(text: &str) -> Option<f64> {
    let t = text.trim();
    let end = t
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || ((*c == '-' || *c == '+') && *i == 0)))
        .map_or(t.len(), |(i, _)| i);
    t[..end].parse().ok()
}

/// The stored value of parameter `k` whose text is `text`: a choice by name; otherwise the
/// value whose text is the same, or whose text's number is nearest to the number typed.
pub fn parse_value(info: &NativeFxInfo, k: usize, text: &str) -> Option<i32> {
    let p = info.params.get(k)?;
    let (min, max) = (p.min as i32, p.max as i32);
    let want = text.trim();
    if !p.choices.is_empty() {
        return p
            .choices
            .iter()
            .position(|c| c.eq_ignore_ascii_case(want))
            .map(|i| min + i as i32)
            .or_else(|| want.parse::<i32>().ok().filter(|v| (min..=max).contains(v)));
    }
    let typed = leading_number(want);
    // Long ranges are searched in steps (a value's neighbours have neighbouring texts), then
    // around the nearest step one by one.
    let step = ((max - min) / 20_000).max(1);
    let scan = |from: i32, to: i32, by: i32| -> Result<Option<(f64, i32)>, i32> {
        let mut best: Option<(f64, i32)> = None;
        let mut v = from;
        while v <= to {
            let t = info.value_text(k, v as f32);
            if t.eq_ignore_ascii_case(want) {
                return Err(v);
            }
            if let (Some(a), Some(b)) = (typed, leading_number(&t)) {
                let d = (a - b).abs();
                if best.is_none_or(|(bd, _)| d < bd) {
                    best = Some((d, v));
                }
            }
            v += by;
        }
        Ok(best)
    };
    let coarse = match scan(min, max, step) {
        Err(v) => return Some(v),
        Ok(b) => b,
    };
    if step == 1 {
        return coarse.map(|(_, v)| v);
    }
    let around = coarse.map_or(min, |(_, v)| v);
    match scan((around - step).max(min), (around + step).min(max), 1) {
        Err(v) => Some(v),
        Ok(b) => b.or(coarse).map(|(_, v)| v),
    }
}

impl PlugParams {
    pub fn new(info: &'static NativeFxInfo, instrument: bool) -> Self {
        let pages = crate::pages::pages(info, instrument);
        let mut values = Vec::with_capacity(info.params.len());
        let mut ids = Vec::with_capacity(info.params.len());
        let mut groups = Vec::with_capacity(info.params.len());
        for (k, p) in info.params.iter().enumerate() {
            let range = IntRange::Linear {
                min: p.min as i32,
                max: p.max as i32,
            };
            let param = IntParam::new(p.name, p.default as i32, range)
                .with_value_to_string(Arc::new(move |v| info.value_text(k, v as f32)))
                .with_string_to_value(Arc::new(move |s| parse_value(info, k, s)));
            values.push(param);
            ids.push(format!("p{k}"));
            groups.push(group(&pages, k));
        }
        let size = crate::editor::size(info, instrument, &pages);
        PlugParams {
            info,
            instrument,
            pages,
            editor_state: EguiState::from_size(size.0, size.1),
            values,
            ids,
            groups,
        }
    }
}

// SAFETY: the pointers point into `values`, which is never resized after `new`; nih-plug keeps
// this object in an `Arc` for as long as it uses the pointers.
unsafe impl Params for PlugParams {
    fn param_map(&self) -> Vec<(String, ParamPtr, String)> {
        self.values
            .iter()
            .enumerate()
            .map(|(k, p)| (self.ids[k].clone(), p.as_ptr(), self.groups[k].clone()))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every value's text reads back as that value, for every parameter of every plug-in (a
    /// sample of long ranges).
    #[test]
    fn value_texts_read_back() {
        let all = prowave_engine::native_instruments()
            .iter()
            .map(|i| (i, true))
            .chain(prowave_engine::native_effects().iter().map(|i| (i, false)));
        for (info, instrument) in all {
            let params = PlugParams::new(info, instrument);
            assert_eq!(params.param_map().len(), info.params.len());
            for (k, p) in info.params.iter().enumerate() {
                let (min, max) = (p.min as i32, p.max as i32);
                let step = ((max - min) / 40).max(1);
                let mut v = min;
                while v <= max {
                    let text = info.value_text(k, v as f32);
                    let back = parse_value(info, k, &text);
                    // Two values may print alike ("0.0" for 0 and 1 of a fine knob): then the
                    // text must at least be the text of the value read back.
                    let ok = back == Some(v)
                        || back.is_some_and(|b| info.value_text(k, b as f32) == text);
                    assert!(ok, "{} {} {v} {text:?} -> {back:?}", info.id, p.name);
                    v += step;
                }
            }
        }
    }
}
