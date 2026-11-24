//! The smoothing of the matching EQ: SciPy's cubic `interp1d` (a not-a-knot cubic spline,
//! what `make_interp_spline(x, y, k=3)` builds) and statsmodels' LOWESS with `it=0` and a
//! `delta` (`statsmodels/nonparametric/_smoothers_lowess.pyx`, ported line for line).
#![allow(clippy::needless_range_loop)]

/// A not-a-knot cubic spline through `(x[i], y[i])`, `x` strictly increasing (at least 4
/// points). Outside `x` it carries on the end pieces (SciPy's `fill_value="extrapolate"`).
pub struct Spline {
    x: Vec<f64>,
    y: Vec<f64>,
    /// Second derivative at each point.
    m: Vec<f64>,
}

impl Spline {
    pub fn new(x: &[f64], y: &[f64]) -> Self {
        let n = x.len();
        assert!(
            n >= 4 && y.len() == n,
            "a cubic spline needs 4 points or more"
        );
        let h: Vec<f64> = (0..n - 1).map(|i| x[i + 1] - x[i]).collect();
        let d: Vec<f64> = (1..n - 1)
            .map(|i| 6.0 * ((y[i + 1] - y[i]) / h[i] - (y[i] - y[i - 1]) / h[i - 1]))
            .collect();
        // Unknowns m[1..n-1] (k = i - 1). Row k: h[i-1] m[i-1] + 2(h[i-1]+h[i]) m[i] + h[i] m[i+1].
        // Not-a-knot puts m[0] = ((h0+h1) m1 - h0 m2) / h1 and the mirror at the far end, which
        // keeps the system tridiagonal in m[1..n-1].
        let k = n - 2;
        let mut sub = vec![0.0; k];
        let mut dia = vec![0.0; k];
        let mut sup = vec![0.0; k];
        let mut rhs = d.clone();
        for r in 0..k {
            let i = r + 1;
            sub[r] = h[i - 1];
            dia[r] = 2.0 * (h[i - 1] + h[i]);
            sup[r] = h[i];
        }
        let (h0, h1) = (h[0], h[1]);
        dia[0] += h0 * (h0 + h1) / h1;
        sup[0] -= h0 * h0 / h1;
        sub[0] = 0.0;
        let (ha, hb) = (h[n - 3], h[n - 2]);
        // m[n-1] = ((ha+hb) m[n-2] - hb m[n-3]) / ha
        dia[k - 1] += hb * (ha + hb) / ha;
        sub[k - 1] -= hb * hb / ha;
        sup[k - 1] = 0.0;
        // Thomas.
        for r in 1..k {
            let w = sub[r] / dia[r - 1];
            dia[r] -= w * sup[r - 1];
            rhs[r] -= w * rhs[r - 1];
        }
        let mut inner = vec![0.0; k];
        inner[k - 1] = rhs[k - 1] / dia[k - 1];
        for r in (0..k - 1).rev() {
            inner[r] = (rhs[r] - sup[r] * inner[r + 1]) / dia[r];
        }
        let mut m = vec![0.0; n];
        m[1..n - 1].copy_from_slice(&inner);
        m[0] = ((h0 + h1) * m[1] - h0 * m[2]) / h1;
        m[n - 1] = ((ha + hb) * m[n - 2] - hb * m[n - 3]) / ha;
        Self {
            x: x.to_vec(),
            y: y.to_vec(),
            m,
        }
    }

    /// The spline at `t`.
    pub fn at(&self, t: f64) -> f64 {
        let n = self.x.len();
        // The piece: the last i with x[i] <= t, kept inside 0..n-1.
        let i = match self.x.partition_point(|v| *v <= t) {
            0 => 0,
            p => (p - 1).min(n - 2),
        };
        let (x0, x1) = (self.x[i], self.x[i + 1]);
        let h = x1 - x0;
        let (a, b) = (x1 - t, t - x0);
        let (m0, m1) = (self.m[i], self.m[i + 1]);
        m0 * a * a * a / (6.0 * h)
            + m1 * b * b * b / (6.0 * h)
            + (self.y[i] / h - m0 * h / 6.0) * a
            + (self.y[i + 1] / h - m1 * h / 6.0) * b
    }
}

/// statsmodels' `lowess(y, x, frac, it=0, delta)` for sorted `x`: the fitted `y` at each `x`.
pub fn lowess(y: &[f64], x: &[f64], frac: f64, delta: f64) -> Vec<f64> {
    let n = x.len();
    let mut k = (frac * n as f64 + 1e-10) as usize;
    k = k.clamp(2, n.max(2)).min(n);
    let mut y_fit = vec![0.0; n];
    if n == 0 {
        return y_fit;
    }
    let mut weights = vec![0.0; n];
    let mut i = 0usize;
    let mut last_fit_i: isize = -1;
    let mut left = 0usize;
    let mut right = k;
    loop {
        let xval = x[i];
        // update_neighborhood
        while right < n && xval > (x[left] + x[right]) / 2.0 {
            left += 1;
            right += 1;
        }
        let radius = (xval - x[left]).max(x[right - 1] - xval);
        // calculate_weights
        let mut sum = 0.0;
        let mut nonzero = 0;
        for j in left..right {
            let dist = (x[j] - xval).abs() / radius;
            let c = dist * dist * dist;
            let o = 1.0 - c;
            let w = o * o * o;
            weights[j] = w;
            sum += w;
            if w > 1e-12 {
                nonzero += 1;
            }
        }
        // calculate_y_fit
        if nonzero < 2 {
            y_fit[i] = y[i];
        } else {
            for j in left..right {
                weights[j] /= sum;
            }
            let mut swx = 0.0;
            for j in left..right {
                swx += weights[j] * x[j];
            }
            let mut sq = 0.0;
            for j in left..right {
                let d = x[j] - swx;
                sq += weights[j] * d * d;
            }
            let sq = sq.max(1e-12);
            let mut acc = 0.0;
            for j in left..right {
                let p = weights[j] * (1.0 + (xval - swx) * (x[j] - swx) / sq);
                acc += p * y[j];
            }
            y_fit[i] = acc;
        }
        // interpolate_skipped_fits
        if last_fit_i < i as isize - 1 {
            let last = last_fit_i as usize;
            let denom = x[i] - x[last];
            for j in last + 1..i {
                let a = (x[j] - x[last]) / denom;
                y_fit[j] = a * y_fit[i] + (1.0 - a) * y_fit[last];
            }
        }
        // update_indices (Python's for-loop leaves its variable on the last value)
        let mut last = i;
        let cut = x[last] + delta;
        let mut kk = last;
        for t in last + 1..n {
            kk = t;
            if x[t] > cut {
                break;
            }
            if x[t] == x[last] {
                y_fit[t] = y_fit[last];
                last = t;
            }
        }
        last_fit_i = last as isize;
        i = (kk.saturating_sub(1)).max(last + 1);
        if last_fit_i >= n as isize - 1 {
            break;
        }
    }
    y_fit
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_spline_is_exact_on_a_cubic() {
        // Not-a-knot reproduces any cubic exactly, inside and outside its points.
        let f = |t: f64| 0.5 * t * t * t - 2.0 * t * t + t - 3.0;
        let x: Vec<f64> = (0..20).map(|i| (i as f64 * 0.37).powf(1.3)).collect();
        let y: Vec<f64> = x.iter().map(|t| f(*t)).collect();
        let s = Spline::new(&x, &y);
        for t in [-0.4, 0.0, 0.11, 1.0, 2.5, 5.0, 7.9, 9.0] {
            assert!(
                (s.at(t) - f(t)).abs() < 1e-9,
                "{t}: {} vs {}",
                s.at(t),
                f(t)
            );
        }
    }

    #[test]
    fn lowess_of_a_line_is_the_line() {
        let n = 500;
        let x: Vec<f64> = (0..n).map(|i| i as f64 / (n - 1) as f64).collect();
        let y: Vec<f64> = x.iter().map(|t| 3.0 * t - 1.0).collect();
        for delta in [0.0, 0.01] {
            let fit = lowess(&y, &x, 0.1, delta);
            for (a, b) in fit.iter().zip(&y) {
                assert!((a - b).abs() < 1e-9);
            }
        }
    }
}
