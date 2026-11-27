//! Real-time audio safety guards ported from Marbble:
//! - `contain`: silences NaNs and clamps runaway samples (+60 dBFS) so an unstable plug-in
//!   never poisons downstream filters or speakers.
//! - `NoDenormals`: RAII flush-to-zero / denormals-are-zero guard for x86_64 and aarch64.
//!
//! Ported from Marbble (<https://github.com/JackMechem/marbble>),
//! Copyright (c) 2026 Jack Mechem, MIT License.

pub use marbble_dsp::denormals::NoDenormals;

/// The loudest sample a plug-in may output (+60 dBFS); anything past it is a plug-in blowing up.
pub const RUNAWAY: f32 = 1000.0;

/// Silences NaNs and clamps infinite or runaway samples. Returns `true` if any sample was fixed.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn contain(l: &mut [f32], r: &mut [f32]) -> bool {
    let mut bad = false;
    for s in l.iter_mut().chain(r.iter_mut()) {
        if !(s.abs() <= RUNAWAY) {
            bad = true;
            *s = if s.is_nan() {
                0.0
            } else {
                s.clamp(-RUNAWAY, RUNAWAY)
            };
        }
    }
    bad
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_no_alloc::assert_no_alloc;

    #[test]
    fn contain_silences_nans_and_clamps_runaways_without_allocating() {
        let mut l = [0.5, f32::NAN, 5000.0, -0.25];
        let mut r = [-0.5, f32::INFINITY, f32::NEG_INFINITY, 0.25];
        let fixed = assert_no_alloc(|| {
            let _guard = NoDenormals::new();
            contain(&mut l, &mut r)
        });
        assert!(fixed);
        assert_eq!(l, [0.5, 0.0, RUNAWAY, -0.25]);
        assert_eq!(r, [-0.5, RUNAWAY, -RUNAWAY, 0.25]);
        assert!(!contain(&mut l, &mut r));
    }
}
