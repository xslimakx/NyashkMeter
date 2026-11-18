//! DSP helpers: biquads (RBJ), smoothing, envelope followers, 2x oversampling, saturators.
use std::f32::consts::PI;

#[inline]
pub fn db_to_lin(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}
#[inline]
pub fn lin_to_db(x: f32) -> f32 {
    20.0 * (x.abs() + 1e-12).log10()
}

/// Flush tiny values to zero (denormals).
#[inline]
pub fn flush(x: f32) -> f32 {
    if x.abs() < 1e-20 {
        0.0
    } else {
        x
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Coef {
    pub b0: f32,
    pub b1: f32,
    pub b2: f32,
    pub a1: f32,
    pub a2: f32,
}

impl Coef {
    pub const THRU: Coef = Coef { b0: 1.0, b1: 0.0, b2: 0.0, a1: 0.0, a2: 0.0 };

    fn norm(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Coef {
        Coef { b0: (b0 / a0) as f32, b1: (b1 / a0) as f32, b2: (b2 / a0) as f32, a1: (a1 / a0) as f32, a2: (a2 / a0) as f32 }
    }

    pub fn peaking(fs: f32, f: f32, gain_db: f32, q: f32) -> Coef {
        let f = (f as f64).clamp(10.0, fs as f64 * 0.49);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let w = 2.0 * std::f64::consts::PI * f / fs as f64;
        let al = w.sin() / (2.0 * q as f64);
        let c = w.cos();
        Self::norm(1.0 + al * a, -2.0 * c, 1.0 - al * a, 1.0 + al / a, -2.0 * c, 1.0 - al / a)
    }

    /// Peaking EQ like `peaking`, but with the gain at Nyquist matched to the analog bell
    /// (Orfanidis, "Digital parametric equalizer design with prescribed Nyquist-frequency gain",
    /// 1997): high bells keep their analog shape instead of being squeezed towards Nyquist
    /// ("cramping"). Same centre, gain and half-gain bandwidth as `peaking` lower down.
    pub fn peaking_matched(fs: f32, f: f32, gain_db: f32, q: f32) -> Coef {
        if gain_db.abs() < 0.01 {
            return Coef::THRU;
        }
        let pi = std::f64::consts::PI;
        let f = (f as f64).clamp(10.0, fs as f64 * 0.49);
        let w0 = 2.0 * pi * f / fs as f64;
        let dw = w0 / q.max(0.05) as f64;
        let g0 = 1.0f64;
        let g = 10f64.powf(gain_db as f64 / 20.0);
        let gb = g.sqrt(); // half the gain in dB marks the bandwidth
        let (g2, gb2, g02) = (g * g, gb * gb, g0 * g0);
        // analog gain at Nyquist
        let r = (gb2 - g02) / (g2 - gb2);
        let num = g02 * (w0 * w0 - pi * pi).powi(2) + g2 * pi * pi * dw * dw * r;
        let den = (w0 * w0 - pi * pi).powi(2) + pi * pi * dw * dw * r;
        let g1 = (num / den).sqrt();
        let ff = (g2 - gb2).abs();
        let g00 = (g2 - g02).abs();
        let f00 = (gb2 - g02).abs();
        let g01 = (g2 - g0 * g1).abs();
        let g11 = (g2 - g1 * g1).abs();
        let f01 = (gb2 - g0 * g1).abs();
        let f11 = (gb2 - g1 * g1).abs();
        let w2 = (g11 / g00).sqrt() * (w0 / 2.0).tan().powi(2);
        let dwt = (1.0 + (f00 / f11).sqrt() * w2) * (dw / 2.0).tan();
        let c = f11 * dwt * dwt - 2.0 * w2 * (f01 - (f00 * f11).sqrt());
        let d = 2.0 * w2 * (g01 - (g00 * g11).sqrt());
        let aa = ((c + d) / ff).max(0.0).sqrt();
        let bb = ((g2 * c + gb2 * d) / ff).max(0.0).sqrt();
        let n0 = 1.0 + w2 + aa;
        let co = Coef {
            b0: ((g1 + g0 * w2 + bb) / n0) as f32,
            b1: (-2.0 * (g1 - g0 * w2) / n0) as f32,
            b2: ((g1 + g0 * w2 - bb) / n0) as f32,
            a1: (-2.0 * (1.0 - w2) / n0) as f32,
            a2: ((1.0 + w2 - aa) / n0) as f32,
        };
        if [co.b0, co.b1, co.b2, co.a1, co.a2].iter().all(|v| v.is_finite()) {
            co
        } else {
            Self::peaking(fs, f as f32, gain_db, q)
        }
    }

    pub fn low_shelf(fs: f32, f: f32, gain_db: f32, q: f32) -> Coef {
        let f = (f as f64).clamp(10.0, fs as f64 * 0.49);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let w = 2.0 * std::f64::consts::PI * f / fs as f64;
        let (s, c) = (w.sin(), w.cos());
        let al = s / (2.0 * q as f64);
        let sa = 2.0 * a.sqrt() * al;
        Self::norm(
            a * ((a + 1.0) - (a - 1.0) * c + sa),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - sa),
            (a + 1.0) + (a - 1.0) * c + sa,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - sa,
        )
    }

    pub fn high_shelf(fs: f32, f: f32, gain_db: f32, q: f32) -> Coef {
        let f = (f as f64).clamp(10.0, fs as f64 * 0.49);
        let a = 10f64.powf(gain_db as f64 / 40.0);
        let w = 2.0 * std::f64::consts::PI * f / fs as f64;
        let (s, c) = (w.sin(), w.cos());
        let al = s / (2.0 * q as f64);
        let sa = 2.0 * a.sqrt() * al;
        Self::norm(
            a * ((a + 1.0) + (a - 1.0) * c + sa),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - sa),
            (a + 1.0) - (a - 1.0) * c + sa,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - sa,
        )
    }

    pub fn highpass(fs: f32, f: f32, q: f32) -> Coef {
        let f = (f as f64).clamp(5.0, fs as f64 * 0.49);
        let w = 2.0 * std::f64::consts::PI * f / fs as f64;
        let al = w.sin() / (2.0 * q as f64);
        let c = w.cos();
        Self::norm((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al)
    }

    pub fn lowpass(fs: f32, f: f32, q: f32) -> Coef {
        let f = (f as f64).clamp(5.0, fs as f64 * 0.49);
        let w = 2.0 * std::f64::consts::PI * f / fs as f64;
        let al = w.sin() / (2.0 * q as f64);
        let c = w.cos();
        Self::norm((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al)
    }

    /// First-order high-pass (6 dB/oct), bilinear.
    pub fn highpass1(fs: f32, f: f32) -> Coef {
        let k = (std::f64::consts::PI * (f as f64).clamp(5.0, fs as f64 * 0.49) / fs as f64).tan();
        let b0 = 1.0 / (1.0 + k);
        Coef { b0: b0 as f32, b1: (-b0) as f32, b2: 0.0, a1: ((k - 1.0) / (1.0 + k)) as f32, a2: 0.0 }
    }

    /// First-order low-pass (6 dB/oct), bilinear.
    pub fn lowpass1(fs: f32, f: f32) -> Coef {
        let k = (std::f64::consts::PI * (f as f64).clamp(5.0, fs as f64 * 0.49) / fs as f64).tan();
        let b0 = k / (1.0 + k);
        Coef { b0: b0 as f32, b1: b0 as f32, b2: 0.0, a1: ((k - 1.0) / (1.0 + k)) as f32, a2: 0.0 }
    }

    /// Band-pass with constant 0 dB peak gain.
    pub fn bandpass(fs: f32, f: f32, q: f32) -> Coef {
        let f = (f as f64).clamp(5.0, fs as f64 * 0.49);
        let w = 2.0 * std::f64::consts::PI * f / fs as f64;
        let al = w.sin() / (2.0 * q as f64);
        let c = w.cos();
        Self::norm(al, 0.0, -al, 1.0 + al, -2.0 * c, 1.0 - al)
    }

    /// Complex response (re, im) at `f` Hz.
    pub fn response(&self, fs: f32, f: f32) -> (f32, f32) {
        let w = 2.0 * std::f64::consts::PI * f as f64 / fs as f64;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let nr = self.b0 as f64 + self.b1 as f64 * c1 + self.b2 as f64 * c2;
        let ni = -(self.b1 as f64 * s1 + self.b2 as f64 * s2);
        let dr = 1.0 + self.a1 as f64 * c1 + self.a2 as f64 * c2;
        let di = -(self.a1 as f64 * s1 + self.a2 as f64 * s2);
        let den = (dr * dr + di * di).max(1e-30);
        (((nr * dr + ni * di) / den) as f32, ((ni * dr - nr * di) / den) as f32)
    }

    /// Magnitude response in dB at `f` Hz.
    pub fn response_db(&self, fs: f32, f: f32) -> f32 {
        let w = 2.0 * std::f64::consts::PI * f as f64 / fs as f64;
        let (c1, s1, c2, s2) = (w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin());
        let nr = self.b0 as f64 + self.b1 as f64 * c1 + self.b2 as f64 * c2;
        let ni = -(self.b1 as f64 * s1 + self.b2 as f64 * s2);
        let dr = 1.0 + self.a1 as f64 * c1 + self.a2 as f64 * c2;
        let di = -(self.a1 as f64 * s1 + self.a2 as f64 * s2);
        let num = nr * nr + ni * ni;
        let den = (dr * dr + di * di).max(1e-30);
        (10.0 * (num / den).max(1e-30).log10()) as f32
    }
}

/// Direct-form-II transposed state for one channel.
#[derive(Clone, Copy, Default)]
pub struct Bq {
    z1: f32,
    z2: f32,
}

impl Bq {
    #[inline]
    pub fn tick(&mut self, c: &Coef, x: f32) -> f32 {
        let y = c.b0 * x + self.z1;
        self.z1 = flush(c.b1 * x - c.a1 * y + self.z2);
        self.z2 = flush(c.b2 * x - c.a2 * y);
        y
    }
    pub fn reset(&mut self) {
        *self = Bq::default();
    }
}

/// Cascade of biquad sections sharing coefficients, for stereo use `[Chain; 2]`.
#[derive(Clone)]
pub struct Chain<const N: usize> {
    pub st: [Bq; N],
}
impl<const N: usize> Default for Chain<N> {
    fn default() -> Self {
        Self::new()
    }
}
impl<const N: usize> Chain<N> {
    pub fn new() -> Self {
        Self { st: [Bq::default(); N] }
    }
    #[inline]
    pub fn tick(&mut self, c: &[Coef; N], x: f32) -> f32 {
        let mut y = x;
        for i in 0..N {
            y = self.st[i].tick(&c[i], y);
        }
        y
    }
    pub fn reset(&mut self) {
        for s in self.st.iter_mut() {
            s.reset();
        }
    }
}

/// Butterworth Q values for an even-order cascade (order = 2 * N).
pub fn butter_q<const N: usize>() -> [f32; N] {
    let mut q = [0.0; N];
    let order = (2 * N) as f32;
    for (k, v) in q.iter_mut().enumerate() {
        *v = 1.0 / (2.0 * ((PI * (2.0 * k as f32 + 1.0)) / (2.0 * order)).cos());
    }
    q
}

/// One-pole smoother for control values.
#[derive(Clone, Copy)]
pub struct Smooth {
    pub v: f32,
    a: f32,
}
impl Smooth {
    pub fn new(v: f32, fs: f32, ms: f32) -> Self {
        Self { v, a: (-1.0 / (ms * 0.001 * fs)).exp() }
    }
    #[inline]
    pub fn tick(&mut self, target: f32) -> f32 {
        self.v = target + (self.v - target) * self.a;
        self.v
    }
}

#[inline]
pub fn time_coef(ms: f32, fs: f32) -> f32 {
    (-1.0 / (ms.max(0.001) * 0.001 * fs)).exp()
}

/// Peak envelope with separate attack / release (in the linear domain).
#[derive(Clone, Copy, Default)]
pub struct Env {
    pub v: f32,
}
impl Env {
    #[inline]
    pub fn tick(&mut self, x: f32, att: f32, rel: f32) -> f32 {
        let x = x.abs();
        let c = if x > self.v { att } else { rel };
        self.v = x + (self.v - x) * c;
        self.v = flush(self.v);
        self.v
    }
}

/// 2x oversampler for non-linear processing (8th-order Butterworth anti-imaging / anti-aliasing).
#[derive(Clone)]
pub struct Os2 {
    up: [HalfBand; 2],
    down: [HalfBand; 2],
}

/// Polyphase half-band coefficients (two paths of first-order all-passes in z^2), designed for
/// 100 dB rejection with a transition band of 0.02 x the oversampled rate (de Soras' method):
/// flat to 20 kHz at 44.1 kHz, every image above 24 kHz at least 100 dB down.
const HB: [f64; 10] = [
    0.038198144521241255, 0.14184841446681049, 0.28432674923434886, 0.43650058144942716, 0.577049051804713,
    0.6955241005123943, 0.790200596391607, 0.8644657999046814, 0.9239959278773665, 0.9752865613764005,
];

/// One all-pass path (every other coefficient), running at the base rate.
#[derive(Clone, Copy, Default)]
struct HalfBand {
    x1: [f64; 5],
    y1: [f64; 5],
}
impl HalfBand {
    #[inline]
    fn tick(&mut self, odd: usize, mut x: f64) -> f64 {
        for k in 0..5 {
            let c = HB[2 * k + odd];
            let y = c * (x - self.y1[k]) + self.x1[k];
            self.x1[k] = x;
            self.y1[k] = y;
            x = y;
        }
        x
    }
}

impl Os2 {
    pub fn new(_fs: f32) -> Self {
        Self { up: Default::default(), down: Default::default() }
    }
    pub fn reset(&mut self) {
        *self = Self::new(0.0);
    }
    /// Run `f` at twice the sample rate (IIR half-band up / down: no added latency to report,
    /// the magnitude stays flat to within 0.01 dB up to 20 kHz at 44.1 kHz).
    #[inline]
    pub fn process(&mut self, x: f32, mut f: impl FnMut(f32) -> f32) -> f32 {
        let x = x as f64;
        let a = self.up[0].tick(0, x);
        let b = self.up[1].tick(1, x);
        let ya = f(a as f32) as f64;
        let yb = f(b as f32) as f64;
        let y = 0.5 * (self.down[0].tick(0, yb) + self.down[1].tick(1, ya));
        if y.abs() < 1e-30 {
            0.0
        } else {
            y as f32
        }
    }
}

#[cfg(test)]
mod os_tests {
    use super::*;
    #[test]
    fn os2_is_flat_and_rejects_images() {
        for fs in [44100.0f32, 48000.0] {
            for f in [100.0f32, 1000.0, 10000.0, 18000.0, 20000.0] {
                let mut o = Os2::new(fs);
                let n = 30000;
                let (mut ei, mut eo) = (0.0f64, 0.0f64);
                for i in 0..n {
                    let x = (2.0 * std::f32::consts::PI * f * i as f32 / fs).sin();
                    let y = o.process(x, |v| v);
                    if i > n / 2 {
                        ei += (x * x) as f64;
                        eo += (y * y) as f64;
                    }
                }
                let g = 10.0 * (eo / ei).log10();
                assert!(g.abs() < 0.02, "fs {fs} f {f}: {g} dB");
            }
        }
        // a hard clipper at 2x: a 15 kHz tone at 44.1 kHz makes a 3rd harmonic at 45 kHz, which
        // must not fold back to 0.9 kHz audibly (it lands at 88.2 - 45 = 43.2 kHz, then filtered)
        let fs = 44100.0f32;
        let mut o = Os2::new(fs);
        let n = 44100usize;
        let mut y = vec![0.0f32; n];
        for i in 0..n {
            let x = 0.9 * (2.0 * std::f32::consts::PI * 15000.0 * i as f32 / fs).sin();
            y[i] = o.process(x, |v| (v * 3.0).tanh() / 3.0);
        }
        let amp = |g: f32| {
            let (mut re, mut im) = (0.0f64, 0.0f64);
            for (k, &v) in y[n / 2..].iter().enumerate() {
                let w = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * k as f64 / (n / 2) as f64).cos();
                let ph = 2.0 * std::f64::consts::PI * g as f64 * (k + n / 2) as f64 / fs as f64;
                re += w * v as f64 * ph.cos();
                im += w * v as f64 * ph.sin();
            }
            (re * re + im * im).sqrt()
        };
        let (s, al) = (amp(15000.0), amp(44100.0 * 2.0 - 45000.0 - 44100.0)); // 43.2k -> folds to 900 Hz at 1x
        let r = 20.0 * (al / s).log10();
        println!("aliased 3rd harmonic: {r:.1} dB");
        assert!(r < -80.0, "aliasing {r}");
    }
}

// ----------------------------------------------------------------- saturators

/// Soft saturator, symmetric (odd harmonics).
#[inline]
pub fn sat_tanh(x: f32) -> f32 {
    let x = x.clamp(-6.0, 6.0);
    let x2 = x * x;
    // rational tanh approximation
    x * (27.0 + x2) / (27.0 + 9.0 * x2)
}

/// Asymmetric saturator: adds even harmonics (tube-like). `bias` 0..0.5
#[inline]
pub fn sat_asym(x: f32, bias: f32) -> f32 {
    let y = sat_tanh(x + bias) - sat_tanh(bias);
    // restore unity small-signal gain
    y / (1.0 - sat_tanh(bias).powi(2)).max(0.2)
}

#[cfg(test)]
mod matched_tests {
    use super::*;
    /// analog bell (RBJ prototype) magnitude in dB
    fn analog_db(f0: f64, g_db: f64, q: f64, f: f64) -> f64 {
        let a = 10f64.powf(g_db / 40.0);
        let (w0, w) = (f0, f);
        let re_n = w0 * w0 - w * w;
        let im_n = w * w0 * a / q;
        let im_d = w * w0 / (a * q);
        10.0 * ((re_n * re_n + im_n * im_n) / (re_n * re_n + im_d * im_d)).log10()
    }
    #[test]
    fn matched_bell_follows_the_analog_shape() {
        let fs = 48000.0f32;
        for (f0, g, q) in [(1000.0f32, 12.0f32, 1.0f32), (12000.0, 12.0, 1.0), (15000.0, -9.0, 2.0), (8000.0, 6.0, 0.5), (300.0, -12.0, 4.0)] {
            let c = Coef::peaking_matched(fs, f0, g, q);
            let mut worst = 0.0f64;
            let mut f = 20.0f32;
            while f < 23900.0 {
                let d = c.response_db(fs, f) as f64 - analog_db(f0 as f64, g as f64, q as f64, f as f64);
                worst = worst.max(d.abs());
                f *= 1.03;
            }
            let at = c.response_db(fs, f0);
            println!("f0 {f0} g {g} q {q}: at f0 {at:.2} dB, worst deviation from analog {worst:.2} dB");
            assert!((at - g).abs() < 0.2, "gain at f0 {at}");
            assert!(worst < 1.0, "shape {worst}");
        }
    }
}
