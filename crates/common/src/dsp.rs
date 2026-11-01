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
    c: [Coef; 4],
    up: Chain<4>,
    down: Chain<4>,
}
impl Os2 {
    pub fn new(fs: f32) -> Self {
        let fs2 = fs * 2.0;
        let q = butter_q::<4>();
        let f = (fs * 0.5 * 0.92).min(fs * 0.46);
        let c = [Coef::lowpass(fs2, f, q[0]), Coef::lowpass(fs2, f, q[1]), Coef::lowpass(fs2, f, q[2]), Coef::lowpass(fs2, f, q[3])];
        Self { c, up: Chain::new(), down: Chain::new() }
    }
    pub fn reset(&mut self) {
        self.up.reset();
        self.down.reset();
    }
    /// Run `f` at twice the sample rate.
    #[inline]
    pub fn process(&mut self, x: f32, mut f: impl FnMut(f32) -> f32) -> f32 {
        let a = self.up.tick(&self.c, x * 2.0);
        let b = self.up.tick(&self.c, 0.0);
        let ya = f(a);
        let yb = f(b);
        let da = self.down.tick(&self.c, ya);
        let _ = self.down.tick(&self.c, yb);
        da
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
